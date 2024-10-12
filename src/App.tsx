import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from '@tauri-apps/api/event';
import "./App.css";
import { Box, Button, CircularProgress, Container, CssBaseline, Stack, Typography } from "@mui/material";
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
class StatusMessage {
  public message: string = "";
}

function App() {
  const [installLocation, setInstallLocation] = useState("");
  const [installedVersion, setInstalledVersion] = useState("None");
  const [availableVersion, setAvailableVersion] = useState("Checking...");
  const [statusMessage, setStatusMessage] = useState("status here");
  const [logBuffer, setLogBuffer] = useState(["App log here\n", <br key="32445" />]);
  const [loaded, setLoaded] = useState(false);
  const [checkingIntegrity, setCheckingIntegrity] = useState(false);
  const [launching, setLaunching] = useState(false);


  async function appendLog(message: string) {
    setStatusMessage(message);
    const stamp = new Date().toLocaleTimeString();
    setLogBuffer(old => [message, <br key={stamp} />, ...old]);
  }

  async function loadInstallLocation() {
    setInstallLocation(await invoke("load_install_location"));
  }

  async function getAvailableVersion() {
    setAvailableVersion(await invoke("get_available_version"));
  }

  async function getInstalledVersion() {
    // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
    setInstalledVersion(await invoke("get_installed_version"));
  }

  async function changeStarboundLocation() {
    const file = await openDialog({
      multiple: false,
      directory: true,
    });
    
    // did the user cancel?
    if(file === undefined || file === null) {
      return;
    }

    await invoke("set_install_location", { location: file });
    await refresh();
  }

  async function update() {
    await invoke("update");
    await getInstalledVersion();
  }

  async function launch() {
    setLaunching(true);
    await invoke("launch");
    setLaunching(false);
  }

  async function checkIntegrity() {
    setCheckingIntegrity(true);
    await invoke("check_integrity");
    setCheckingIntegrity(false);
    appendLog("Integrity check completed.");
  }

  async function selfUpdate() {
    appendLog("Checking for updates...");
    const update = await check();
    if (update) {
      appendLog(
        `found update ${update.version} from ${update.date} with notes ${update.body}`
      );
      let downloaded = 0;
      let contentLength = 0;
      // alternatively we could also call update.download() and update.install() separately
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            contentLength = event.data.contentLength ?? 0;
            appendLog(`started downloading ${event.data.contentLength} bytes`);
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            appendLog(`downloaded ${downloaded} from ${contentLength}`);
            break;
          case 'Finished':
            appendLog('download finished');
            break;
        }
      });

      console.log('update installed');
      await relaunch();
    }
    appendLog("Ready");
  }

  async function init() {
    if (!loaded) {
      setLoaded(true);
      await listen<StatusMessage>('status', (event) => {
        console.log("got event ", event);
        setStatusMessage(event.payload.message);
      });
      await listen<StatusMessage>('log', (event) => {
        console.log("got event ", event);
        appendLog(event.payload.message);
      });
      await refresh();
    }
  }

  async function refresh() {
    await loadInstallLocation();
    await getInstalledVersion();
    await getAvailableVersion();
  }

  useEffect(() => {
    const startup = async function() {
      try {
        await selfUpdate();
      } catch (e) {
        appendLog(`Error checking for updates: ${e}`);
      }
      await init();
    };

    startup();
  }, []);

  return (
    <Container>
      <CssBaseline />
      <Box p={3} >
        <Typography variant="h2">Base10 Starbound Modpack</Typography>
        <Stack direction="column" spacing={2}>

          <Stack direction="row" spacing={2} alignItems="center">
            <Typography>Starbound location: {installLocation}</Typography>
            <Button onClick={() => changeStarboundLocation()}>Change</Button>
          </Stack>
          <Stack direction="row" spacing={2} alignItems="center">
            <Typography variant="subtitle2">Modpack Version</Typography>
            <Typography>Installed: {installedVersion}</Typography>
            <Typography>Available: {availableVersion}</Typography>
            {installedVersion != availableVersion ? <Button onClick={() => update()}>Update</Button> : <span>- Up to date!</span>}
            <Button onClick={() => getAvailableVersion()}>Check for Updates</Button>
          </Stack>

          <Button variant="contained" disabled={launching} onClick={() => launch()}>Launch!</Button>


          <Button onClick={() => checkIntegrity()}>
            Check mod files integrity
            {checkingIntegrity && <CircularProgress />}
          </Button>
        </Stack>
        <Typography>
          {statusMessage}
        </Typography>
        <Box sx={{ backgroundColor: "#322222", height: "350px", overflow: "scroll" }}>
          {logBuffer}
        </Box>
        <Button onClick={() => setLogBuffer([])}>Clear Log</Button>
      </Box>
    </Container>
  );
}

export default App;
