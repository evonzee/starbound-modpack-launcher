# Starbound Modpack Launcher

This launcher was built for a modpack in use on my personal Starbound server, which then merged with Grayles (which may be reborn).  

It can install the modpack in a clean Starbound 1.4.4 folder.  It deploys the mods (and makes its own storage) separate from your existing config, so it won't interfere with any other custom mod configuration you might have.
It also only downloads updated mods when new modpacks are released (~monthly)... you don't have to re-download the whole thing.
There's also a builtin integrity checker that can scan all modfiles to confirm they were all downloaded OK (and will automatically download replacements if they are found to be corrupt).

## Instructions for use
- Install and run the launcher
- select your starbound folder
- Click the update button to update the modpack
- Assuming everything went well, click launch!
- Connect to the Grayles server at grayles.com, default starbound port.  Asset mismatch is not allowed!

# Developing this launcher

This template should help get you started developing with Tauri, React and Typescript in Vite.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Running in development

Just open a shell in the root of the project, and run `npm run tauri dev`

## Building for production

We want to support cross-platform builds, Linux and Windows. So you'll need to figure that out.

## Making a release

- Make a release branch
- Update the version number in 
    - /package.json
    - /src-tauri/cargo.toml
    - /src-tauri/tauri.conf.json
- land the release branch to main
- tag and push to main
- github actions will create the release automatically