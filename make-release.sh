#!/bin/bash

type=$1
if [ -z "$type" ]; then
  echo "Usage: $0 <major|minor|patch>"
  exit 1
fi

cd src-tauri
cargo bump $type
cargo update
git add . && git commit -m "bump tauri $type version"

npm version $type