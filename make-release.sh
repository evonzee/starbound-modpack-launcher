#!/bin/bash

type=$1
if [ -z "$type" ]; then
  echo "Usage: $0 <major|minor|patch>"
  exit 1
fi

npm version $type
cd src-tauri
cargo bump $type
git commit --amend