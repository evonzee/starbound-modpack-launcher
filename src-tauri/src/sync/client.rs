//! Thin typed wrapper over the sync server's HTTP API.
use std::{fmt, time::Duration};

use reqwest::{header, Client, Response, StatusCode};
use serde::{Deserialize, Serialize};

use super::plan::{Precond, RemoteMeta};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub machine: String,
    pub expires_at: i64,
}

#[derive(Debug)]
pub enum SyncError {
    Unauthorized,
    /// 412; carries the server's current head, if any.
    Conflict(Option<RemoteMeta>),
    LeaseHeld(Lease),
    Network(String),
    Other(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Unauthorized => write!(f, "sync token rejected (check your settings)"),
            SyncError::Conflict(_) => write!(f, "version conflict"),
            SyncError::LeaseHeld(l) => write!(f, "another machine ({}) is playing", l.machine),
            SyncError::Network(e) => write!(f, "network error: {e}"),
            SyncError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl From<reqwest::Error> for SyncError {
    fn from(e: reqwest::Error) -> Self {
        SyncError::Network(e.without_url().to_string())
    }
}

pub type Res<T> = Result<T, SyncError>;

#[derive(Clone)]
pub struct SyncClient {
    http: Client,
    base: String,
    token: String,
    pub pack: String,
    pub machine: String,
}

#[derive(Deserialize)]
struct Manifest {
    files: Vec<RemoteMeta>,
}
#[derive(Deserialize)]
struct Versions {
    versions: Vec<RemoteMeta>,
}
#[derive(Deserialize)]
struct LeaseBody {
    lease: Option<Lease>,
}
#[derive(Deserialize)]
struct ConflictBody {
    current: Option<RemoteMeta>,
}
#[derive(Deserialize)]
struct HeldBody {
    holder: Lease,
}

pub struct Download {
    pub bytes: Vec<u8>,
    pub sha256: Option<String>,
}

impl SyncClient {
    pub fn new(url: &str, token: &str, pack: &str, machine: &str) -> Res<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
            base: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            pack: pack.to_string(),
            machine: machine.to_string(),
        })
    }

    fn files_url(&self, name: &str) -> String {
        format!("{}/v1/packs/{}/files/{}", self.base, self.pack, name)
    }
    fn lease_url(&self) -> String {
        format!("{}/v1/packs/{}/lease", self.base, self.pack)
    }

    async fn check(&self, resp: Response) -> Res<Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        match status {
            StatusCode::UNAUTHORIZED => Err(SyncError::Unauthorized),
            StatusCode::PRECONDITION_FAILED => {
                let cur = resp.json::<ConflictBody>().await.ok().and_then(|b| b.current);
                Err(SyncError::Conflict(cur))
            }
            StatusCode::CONFLICT => match resp.json::<HeldBody>().await {
                Ok(b) => Err(SyncError::LeaseHeld(b.holder)),
                Err(e) => Err(e.into()),
            },
            s => {
                let body = resp.text().await.unwrap_or_default();
                Err(SyncError::Other(format!("server returned {s}: {body}")))
            }
        }
    }

    fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http.get(url).bearer_auth(&self.token)
    }

    /// Cheap auth + reachability check.
    pub async fn test(&self) -> Res<()> {
        let r = self.get(&format!("{}/v1/packs", self.base)).send().await?;
        self.check(r).await.map(|_| ())
    }

    pub async fn manifest(&self) -> Res<Vec<RemoteMeta>> {
        let r = self
            .get(&format!("{}/v1/packs/{}/files", self.base, self.pack))
            .send()
            .await?;
        Ok(self.check(r).await?.json::<Manifest>().await?.files)
    }

    pub async fn download(&self, name: &str) -> Res<Download> {
        let r = self.get(&self.files_url(name)).send().await?;
        let r = self.check(r).await?;
        let sha256 = r
            .headers()
            .get("x-sha256")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_ascii_lowercase());
        let bytes = r.bytes().await?.to_vec();
        Ok(Download { bytes, sha256 })
    }

    pub async fn upload(
        &self,
        name: &str,
        bytes: Vec<u8>,
        sha256: &str,
        pre: Precond,
        pack_version: Option<&str>,
    ) -> Res<RemoteMeta> {
        let mut req = self
            .http
            .put(self.files_url(name))
            .bearer_auth(&self.token)
            .header("x-machine", &self.machine)
            .header("x-sha256", sha256);
        req = match pre {
            Precond::Absent => req.header(header::IF_NONE_MATCH, "*"),
            Precond::Version(v) => req.header(header::IF_MATCH, format!("\"{v}\"")),
        };
        if let Some(pv) = pack_version {
            req = req.header("x-pack-version", pv);
        }
        let r = req.body(bytes).send().await?;
        Ok(self.check(r).await?.json::<RemoteMeta>().await?)
    }

    pub async fn history(&self, name: &str) -> Res<Vec<RemoteMeta>> {
        let r = self.get(&format!("{}/history", self.files_url(name))).send().await?;
        Ok(self.check(r).await?.json::<Versions>().await?.versions)
    }

    pub async fn restore(&self, name: &str, version: i64) -> Res<RemoteMeta> {
        let r = self
            .http
            .post(format!("{}/restore/{}", self.files_url(name), version))
            .bearer_auth(&self.token)
            .header("x-machine", &self.machine)
            .send()
            .await?;
        Ok(self.check(r).await?.json::<RemoteMeta>().await?)
    }

    pub async fn lease_get(&self) -> Res<Option<Lease>> {
        let r = self.get(&self.lease_url()).send().await?;
        Ok(self.check(r).await?.json::<LeaseBody>().await?.lease)
    }

    /// Acquire or renew. `Err(LeaseHeld)` if another machine has a live lease.
    pub async fn lease_acquire(&self, force: bool) -> Res<()> {
        let r = self
            .http
            .post(self.lease_url())
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "machine": self.machine, "force": force }))
            .send()
            .await?;
        self.check(r).await.map(|_| ())
    }

    pub async fn lease_release(&self) -> Res<()> {
        let r = self
            .http
            .delete(self.lease_url())
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "machine": self.machine }))
            .send()
            .await?;
        self.check(r).await.map(|_| ())
    }
}
