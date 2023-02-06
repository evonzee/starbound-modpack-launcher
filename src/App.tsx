import { useEffect, useState } from "react";
import reactLogo from "./assets/react.svg";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from '@tauri-apps/api/event';
import "./App.css";

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
    await invoke("launch");
  }

  async function checkIntegrity() {
    await invoke("check_integrity");
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
    <div className="container">
      <h1>Welcome to Grayles Starbound Modpack!</h1>

      <p>Starbound location: {installLocation}</p>
      <p>Modpack version installed: {installedVersion}</p>
      <p>
        Modpack version available: {availableVersion} 
        {installedVersion != availableVersion ? <button type="button" onClick={() => update()}>Update</button> : <span> - Up to date!</span> }
      </p>

      <button type="button" onClick={() => changeStarboundLocation()}>Change Starbound location</button>
      <button type="button" onClick={() => getAvailableVersion()}>Check for Updates</button> 
      <button type="button" onClick={() => checkIntegrity()}>Check mod files integrity</button> 
      <button type="button" onClick={() => launch()}>Launch!</button>

      <div>
        { statusMessage }
      </div>
      <pre className="logbox">
        { logBuffer }
      </pre>
      <button type="button" onClick={() => setLogBuffer([])}>Clear Log</button>
    </div>
  );
}

export default App;
