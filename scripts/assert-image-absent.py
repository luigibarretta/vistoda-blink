#!/usr/bin/env python3
"""Fail closed unless GHCR proves the exact version is absent."""

import base64
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

image, version = sys.argv[1:]
if not image.startswith("ghcr.io/") or not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
    raise SystemExit("Expected a GHCR repository and exact release version")
name = image.removeprefix("ghcr.io/")
credentials = f"{os.environ['GITHUB_ACTOR']}:{os.environ['GH_TOKEN']}".encode()
query = urllib.parse.urlencode({"service": "ghcr.io", "scope": f"repository:{name}:pull"})
request = urllib.request.Request(
    f"https://ghcr.io/token?{query}",
    headers={"Authorization": "Basic " + base64.b64encode(credentials).decode()},
)
with urllib.request.urlopen(request, timeout=20) as response:
    token = json.load(response)["token"]
request = urllib.request.Request(
    f"https://ghcr.io/v2/{name}/manifests/{version}",
    method="HEAD",
    headers={
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.oci.image.index.v1+json, application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json",
    },
)
try:
    with urllib.request.urlopen(request, timeout=20):
        raise SystemExit("Immutable version already exists; create a new version")
except urllib.error.HTTPError as error:
    if error.code != 404:
        raise SystemExit(f"Cannot prove version absent: registry HTTP {error.code}") from error
print(f"Registry confirms {image}:{version} is absent")
