import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from '@tauri-apps/api/event';
import "./App.css";
import { Box, Button, CircularProgress, Container, CssBaseline, Stack, Typography } from "@mui/material";

class StatusMessage {
  public message : string = "";
}

function App() {
  const [installLocation, setInstallLocation] = useState("");
  const [installedVersion, setInstalledVersion] = useState("None");
  const [availableVersion, setAvailableVersion] = useState("Checking...");
  const [statusMessage, setStatusMessage] = useState("status here");
  const [logBuffer, setLogBuffer] = useState(["App log here\n", <br/>]);
  const [loaded, setLoaded] = useState(false);
  const [checkingIntegrity, setCheckingIntegrity] = useState(false);
  const [launching, setLaunching] = useState(false);
  
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
    setInstallLocation(await invoke("change_starbound_location"));
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
  }

  async function init() {
    if(!loaded) {
      setLoaded(true);
      await listen<StatusMessage>('status', (event) => {
        console.log("got event ", event);
        setStatusMessage(event.payload.message);
      });
      await listen<StatusMessage>('log', (event) => {
        console.log("got event ", event);
        setStatusMessage(event.payload.message);
        setLogBuffer(old => [event.payload.message, <br/>, ...old]);
      });
      await loadInstallLocation();
      await getInstalledVersion();
      await getAvailableVersion();
    }
  }
  
  init();

  return (
    <Container>
      <CssBaseline/>
      <Box bgcolor={"#fff"} p={3} >
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
            {installedVersion != availableVersion ? <Button onClick={() => update()}>Update</Button> : <span>- Up to date!</span> }
            <Button onClick={() => getAvailableVersion()}>Check for Updates</Button> 
          </Stack>

          <Button variant="contained" disabled={launching} onClick={() => launch()}>Launch!</Button>
                  
          
          <Button onClick={() => checkIntegrity()}>
            Check mod files integrity 
            {checkingIntegrity && <CircularProgress/>}
          </Button> 
        </Stack>
        <Typography>
          { statusMessage }
        </Typography>
        <pre className="logbox">
          { logBuffer }
        </pre>
        <Button onClick={() => setLogBuffer([])}>Clear Log</Button>
    </Box>
    </Container>
  );
}

export default App;
