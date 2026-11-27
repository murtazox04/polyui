#!/usr/bin/env bash
set -euo pipefail

repo="${POLYUI_REPO:-murtazox04/polyui}"
registries=(pypi)

ask_secret() {
  local name="$1" prompt="$2" value
  read -r -s -p "$prompt (empty to skip): " value
  printf '\n'
  if [[ -z "$value" ]]; then
    return 1
  fi
  printf '%s' "$value" | gh secret set "$name" -R "$repo"
  printf 'set %s\n' "$name"
}

ask_file_secret() {
  local name="$1" prompt="$2" path
  read -r -p "$prompt (empty to skip): " path
  if [[ -z "$path" ]]; then
    return 1
  fi
  gh secret set "$name" -R "$repo" < "$path"
  printf 'set %s\n' "$name"
}

printf 'Secrets go to %s. Nothing is printed or written to disk.\n\n' "$repo"

printf 'crates.io: create an API token with publish-new and publish-update under Account Settings, API Tokens on crates.io\n'
if ask_secret CARGO_REGISTRY_TOKEN "crates.io token"; then
  registries+=(crates)
fi

printf '\nnpm: create the organization polyui, then a granular access token with read and write on @polyui under Access Tokens in your npm account\n'
if ask_secret NPM_TOKEN "npm token"; then
  registries+=(npm)
fi

printf '\nMaven Central: generate a user token under your account on central.sonatype.com and export your signing key with gpg --export-secret-keys --armor KEY_ID > key.asc\n'
if ask_secret MAVEN_CENTRAL_USERNAME "Maven Central token user name" \
  && ask_secret MAVEN_CENTRAL_PASSWORD "Maven Central token password" \
  && ask_file_secret SIGNING_KEY "Path to the armored signing key" \
  && ask_secret SIGNING_KEY_PASSWORD "Signing key passphrase"; then
  registries+=(maven)
fi

printf '\npub.dev: the first version has to be published by hand (cd flutter/polyui && flutter pub publish), then enable automated publishing on pub.dev.\n'
read -r -p "Is automated publishing on pub.dev set up? (y/N): " pub
if [[ "$pub" == "y" || "$pub" == "Y" ]]; then
  registries+=(pub)
fi

gh variable set RELEASE_REGISTRIES -R "$repo" --body "${registries[*]}"
printf '\nRELEASE_REGISTRIES is now: %s\n' "${registries[*]}"
