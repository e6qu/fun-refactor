#!/usr/bin/env python3
"""Select a published release and immutable source commit before building assets."""
import base64
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib


def require(condition, message):
    if not condition:
        raise ValueError(message)


def api(repository, path):
    result = subprocess.run(["gh", "api", f"repos/{repository}/{path}"], check=True,
                            capture_output=True, timeout=20)
    require(len(result.stdout) <= 1024**2, "release metadata exceeds 1 MiB")
    return json.loads(result.stdout)


def select(event, requested_tag, created, created_tag, created_version, repository, fetch):
    require(event in {"push", "workflow_dispatch"}, "release publication requires an explicit supported event")
    require(re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository), "invalid release repository")
    if event == "push":
        require(not requested_tag, "automatic publication cannot select an existing release")
        require(created in {"", "false", "true"}, "invalid release-created output")
        if created != "true":
            require(not created_tag and not created_version, "uncreated release has tag or version outputs")
            return {"released": "false", "tag": "", "version": "", "commit": ""}
        tag = created_tag
    else:
        require(not created and not created_tag and not created_version, "manual publication cannot create a release")
        tag = requested_tag
    match = re.fullmatch(r"fun-refactor-v([0-9]+\.[0-9]+\.[0-9]+)", tag)
    require(match, "select an explicit fun-refactor-vMAJOR.MINOR.PATCH tag")
    version = match.group(1)
    if event == "push":
        require(version == created_version, "created release version and tag differ")
    release = fetch(repository, f"releases/tags/{tag}")
    require(release["tag_name"] == tag and release["draft"] is False and release["prerelease"] is False
            and bool(release["published_at"]), "release must already be published")
    require(release["assets"] == [], "release already has assets; refuse to replace downloads")
    if event == "workflow_dispatch":
        latest = fetch(repository, "releases/latest")
        require(latest["id"] == release["id"], "manual completion is limited to the latest published release")
    commit = fetch(repository, f"commits/{tag}")["sha"]
    require(isinstance(commit, str) and re.fullmatch(r"[0-9a-f]{40}", commit), "release tag must resolve to a full commit")
    cargo = fetch(repository, f"contents/Cargo.toml?ref={commit}")
    require(cargo["encoding"] == "base64" and cargo["path"] == "Cargo.toml", "missing release package manifest")
    manifest = tomllib.loads(base64.b64decode("".join(cargo["content"].split()), validate=True).decode())
    require(manifest["package"]["name"] == "fun-refactor" and manifest["package"]["version"] == version,
            "release tag and checked-out package version differ")
    return {"released": "true", "tag": tag, "version": version, "commit": commit}


def main():
    result = select(os.environ["GITHUB_EVENT_NAME"], os.environ["REQUESTED_TAG"],
                    os.environ["CREATED_RELEASE"], os.environ["CREATED_TAG"], os.environ["CREATED_VERSION"],
                    os.environ["GITHUB_REPOSITORY"], api)
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        for key, value in result.items():
            output.write(f"{key}={value}\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
