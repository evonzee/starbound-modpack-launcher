import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Alert, Box, Button, Chip, Collapse, Dialog, DialogActions, DialogContent, DialogTitle,
  FormControlLabel, Stack, Switch, Table, TableBody, TableCell, TableHead, TableRow,
  TextField, Typography,
} from "@mui/material";

interface Settings { url: string; token: string; enabled: boolean; machine: string }
interface RemoteMeta {
  name: string; version: number; sha256: string; size: number; pack_version: string | null;
  updated_by: string; updated_at: number; deleted: boolean;
}
interface FileStatus { name: string; status: string; local_size: number | null; character: string | null; remote: RemoteMeta | null }
interface StatusReport { files: FileStatus[]; lease: { machine: string; expires_at: number } | null; machine: string }
interface Summary { pulled: string[]; pushed: string[]; removed: string[]; conflicts: string[]; blocked: string[]; errors: string[] }

const BADGES: Record<string, { label: string; color: "success" | "info" | "warning" | "error" | "default" }> = {
  "synced": { label: "Synced", color: "success" },
  "ahead": { label: "Local changes", color: "info" },
  "local-only": { label: "Not uploaded", color: "info" },
  "behind": { label: "Server is newer", color: "warning" },
  "remote-only": { label: "Only on server", color: "warning" },
  "needs-update": { label: "Needs newer modpack", color: "error" },
  "conflict": { label: "Conflict", color: "error" },
  "deleted-remotely": { label: "Deleted elsewhere", color: "warning" },
};

const fmtTime = (s: number) => new Date(s * 1000).toLocaleString();
const fmtSize = (n: number | null | undefined) => (n == null ? "-" : `${(n / 1024).toFixed(1)} KB`);

function kind(name: string) { return name.endsWith(".player") ? "Player" : "Ship / world"; }
function charName(files: FileStatus[]) { return files.find((f) => f.character)?.character ?? null; }
function charId(name: string) { return name.slice(0, name.lastIndexOf(".")); }

export default function SyncDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [settings, setSettings] = useState<Settings>({ url: "", token: "", enabled: false, machine: "" });
  const [showSettings, setShowSettings] = useState(false);
  const [report, setReport] = useState<StatusReport | null>(null);
  const [message, setMessage] = useState<{ severity: "info" | "error" | "success"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [historyFor, setHistoryFor] = useState<string | null>(null);
  const [versions, setVersions] = useState<RemoteMeta[]>([]);
  const [confirmRestore, setConfirmRestore] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    setBusy(true);
    try {
      setReport(await invoke<StatusReport>("sync_status"));
      setMessage(null);
    } catch (e) {
      setReport(null);
      setMessage({ severity: "error", text: String(e) });
    }
    setBusy(false);
  }, []);

  useEffect(() => {
    if (!open) return;
    (async () => {
      const s = await invoke<Settings>("sync_get_settings");
      setSettings(s);
      if (!s.url || !s.token) setShowSettings(true);
      else refresh();
    })();
  }, [open, refresh]);

  async function run<T>(fn: () => Promise<T>, ok?: (r: T) => string) {
    setBusy(true);
    try {
      const r = await fn();
      if (ok) setMessage({ severity: "success", text: ok(r) });
    } catch (e) {
      setMessage({ severity: "error", text: String(e) });
    }
    setBusy(false);
  }

  const save = () => run(async () => { await invoke("sync_set_settings", { settings }); await refresh(); }, () => "Settings saved");
  const test = () => run(() => invoke<string>("sync_test_connection", { settings }), (r) => r);
  const syncNow = () => run(async () => {
    const s = await invoke<Summary>("sync_now");
    await refresh();
    return s;
  }, (s) => {
    const parts = [`${s.pulled.length} downloaded`, `${s.pushed.length} uploaded`];
    if (s.conflicts.length) parts.push(`${s.conflicts.length} conflict(s)`);
    if (s.blocked.length) parts.push(`${s.blocked.length} need a newer modpack`);
    if (s.errors.length) parts.push(`errors: ${s.errors.join("; ")}`);
    return parts.join(", ");
  });
  const resolve = (name: string, keep: "local" | "remote") =>
    run(async () => { await invoke("sync_resolve", { name, keep }); await refresh(); }, () => `${name}: resolved`);

  async function openHistory(name: string) {
    setHistoryFor(name);
    setVersions([]);
    setConfirmRestore(null);
    await run(async () => setVersions(await invoke<RemoteMeta[]>("sync_history", { name })));
  }
  const restore = (name: string, version: number) =>
    run(async () => {
      await invoke("sync_restore", { name, version });
      setHistoryFor(null);
      await refresh();
    }, () => `${name}: rolled back to version ${version} (previous local copy backed up)`);

  // group by character id
  const groups = new Map<string, FileStatus[]>();
  (report?.files ?? []).forEach((f) => {
    const id = charId(f.name);
    groups.set(id, [...(groups.get(id) ?? []), f]);
  });

  return (
    <>
      <Dialog open={open} onClose={onClose} fullWidth maxWidth="lg">
        <DialogTitle>Cloud sync</DialogTitle>
        <DialogContent>
          <Stack spacing={2}>
            {message && <Alert severity={message.severity}>{message.text}</Alert>}
            {report?.lease && report.lease.machine !== report.machine && (
              <Alert severity="warning">
                Currently being played on "{report.lease.machine}" (until {fmtTime(report.lease.expires_at)})
              </Alert>
            )}

            <Box>
              <Button size="small" onClick={() => setShowSettings(!showSettings)}>
                {showSettings ? "Hide settings" : "Settings"}
              </Button>
              <Collapse in={showSettings}>
                <Stack spacing={2} sx={{ mt: 1, maxWidth: 480 }}>
                  <FormControlLabel
                    control={<Switch checked={settings.enabled} onChange={(e) => setSettings({ ...settings, enabled: e.target.checked })} />}
                    label="Sync automatically when launching / exiting"
                  />
                  <TextField size="small" label="Server URL" value={settings.url} onChange={(e) => setSettings({ ...settings, url: e.target.value })} />
                  <TextField size="small" label="Token" type="password" value={settings.token} onChange={(e) => setSettings({ ...settings, token: e.target.value })} />
                  <TextField size="small" label="This computer's name" value={settings.machine} onChange={(e) => setSettings({ ...settings, machine: e.target.value })} />
                  <Stack direction="row" spacing={1}>
                    <Button variant="contained" disabled={busy} onClick={save}>Save</Button>
                    <Button disabled={busy} onClick={test}>Test connection</Button>
                  </Stack>
                </Stack>
              </Collapse>
            </Box>

            <Stack direction="row" spacing={1}>
              <Button variant="contained" disabled={busy} onClick={syncNow}>Sync now</Button>
              <Button disabled={busy} onClick={refresh}>Refresh</Button>
            </Stack>

            {report && groups.size === 0 && <Typography>No player or ship files found locally or on the server.</Typography>}
            {groups.size > 0 && (
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>Character</TableCell>
                    <TableCell>Type</TableCell>
                    <TableCell>Status</TableCell>
                    <TableCell>Local</TableCell>
                    <TableCell>Server</TableCell>
                    <TableCell />
                  </TableRow>
                </TableHead>
                <TableBody>
                  {[...groups.entries()].map(([id, files]) =>
                    files.map((f, i) => {
                      const b = BADGES[f.status] ?? { label: f.status, color: "default" as const };
                      return (
                        <TableRow key={f.name}>
                          <TableCell>
                            {i === 0 && (
                              <>
                                <Typography component="span" sx={{ fontWeight: 600 }}>{charName(files) ?? "Unknown character"}</Typography>
                                <Typography variant="caption" sx={{ display: "block", fontFamily: "monospace", color: "text.secondary" }}>{id}</Typography>
                              </>
                            )}
                          </TableCell>
                          <TableCell>{kind(f.name)}</TableCell>
                          <TableCell><Chip size="small" label={b.label} color={b.color} /></TableCell>
                          <TableCell>{fmtSize(f.local_size)}</TableCell>
                          <TableCell>
                            {f.remote
                              ? `v${f.remote.version}, ${f.remote.deleted ? "deleted" : fmtSize(f.remote.size)} · ${f.remote.updated_by}, ${fmtTime(f.remote.updated_at)}`
                              : "-"}
                          </TableCell>
                          <TableCell align="right">
                            {f.status === "conflict" && (
                              <>
                                <Button size="small" disabled={busy} onClick={() => resolve(f.name, "local")}>Keep mine</Button>
                                <Button size="small" disabled={busy || !f.remote} onClick={() => resolve(f.name, "remote")}>Keep server's</Button>
                              </>
                            )}
                            {f.remote && <Button size="small" disabled={busy} onClick={() => openHistory(f.name)}>History</Button>}
                          </TableCell>
                        </TableRow>
                      );
                    })
                  )}
                </TableBody>
              </Table>
            )}
            <Typography variant="caption">
              Conflicts are never resolved automatically; the version you don't keep stays in history (server) or in sync-backups (this computer).
            </Typography>
          </Stack>
        </DialogContent>
        <DialogActions><Button onClick={onClose}>Close</Button></DialogActions>
      </Dialog>

      <Dialog open={historyFor !== null} onClose={() => setHistoryFor(null)} fullWidth maxWidth="md">
        <DialogTitle>History: {historyFor && (report?.files.find((f) => f.name === historyFor)?.character ?? historyFor)} ({historyFor && kind(historyFor)})</DialogTitle>
        <DialogContent>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Version</TableCell><TableCell>When</TableCell><TableCell>From</TableCell>
                <TableCell>Size</TableCell><TableCell>Modpack</TableCell><TableCell />
              </TableRow>
            </TableHead>
            <TableBody>
              {versions.map((v, i) => (
                <TableRow key={v.version}>
                  <TableCell>v{v.version}{i === 0 ? " (current)" : ""}</TableCell>
                  <TableCell>{fmtTime(v.updated_at)}</TableCell>
                  <TableCell>{v.updated_by}</TableCell>
                  <TableCell>{v.deleted ? "deleted" : fmtSize(v.size)}</TableCell>
                  <TableCell>{v.pack_version ?? "-"}</TableCell>
                  <TableCell align="right">
                    {!v.deleted && i > 0 && (confirmRestore === v.version
                      ? <>
                        <Button size="small" color="error" disabled={busy} onClick={() => restore(historyFor!, v.version)}>Confirm rollback</Button>
                        <Button size="small" onClick={() => setConfirmRestore(null)}>Cancel</Button>
                      </>
                      : <Button size="small" disabled={busy} onClick={() => setConfirmRestore(v.version)}>Roll back to this</Button>)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </DialogContent>
        <DialogActions><Button onClick={() => setHistoryFor(null)}>Close</Button></DialogActions>
      </Dialog>
    </>
  );
}
