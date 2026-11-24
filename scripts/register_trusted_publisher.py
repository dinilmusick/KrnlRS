#!/usr/bin/env python3
"""Idempotently register the exact crates.io trusted publisher after initial publication.

Requires a short-lived, scoped bootstrap token only inside the protected
GitHub Actions crates-bootstrap environment. No registry token is logged.
"""
import json
import os
import pathlib
import sys
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request

BASE = "https://crates.io/api/v1"
WORKFLOW = "release-rust.yml"
ENVIRONMENT = "crates-io"


def package_identity():
    with pathlib.Path("Cargo.toml").open("rb") as file:
        package = tomllib.load(file)["package"]
    return package["name"], package["version"]


def request_json(path, token=None, payload=None):
    body = None if payload is None else json.dumps(payload, separators=(",", ":")).encode()
    headers = {"User-Agent": "ULTRST-Rust-release-bootstrap/1", "Accept": "application/json"}
    if body is not None:
        headers["Content-Type"] = "application/json"
    if token is not None:
        headers["Authorization"] = token
    req = urllib.request.Request(BASE + path, data=body, headers=headers, method="POST" if body is not None else "GET")
    try:
        with urllib.request.urlopen(req, timeout=25) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return 404, {}
        detail = exc.read(4096).decode("utf-8", "replace")
        raise RuntimeError("crates.io API returned HTTP %s: %s" % (exc.code, detail)) from None


def assert_published(crate, version):
    path = "/crates/%s/%s" % (urllib.parse.quote(crate, safe=""), urllib.parse.quote(version, safe=""))
    for attempt in range(12):
        status, response = request_json(path)
        if status == 200:
            if response.get("version", {}).get("num") != version:
                raise RuntimeError("crates.io returned a mismatched version")
            return
        if status != 404:
            raise RuntimeError("Unexpected registry response")
        if attempt < 11:
            time.sleep(10)
    raise RuntimeError("Exact version %s@%s is not visible on crates.io" % (crate, version))


def publisher_record(crate, repository):
    owner, repo = repository.split("/", 1)
    if not owner or not repo:
        raise RuntimeError("Invalid GitHub repository")
    return {"crate": crate, "repository_owner": owner, "repository_name": repo,
            "workflow_filename": WORKFLOW, "environment": ENVIRONMENT}


def matches(config, expected):
    return all(config.get(k) == value for k, value in expected.items())


def ensure_publisher(crate, token, repository):
    expected = publisher_record(crate, repository)
    path = "/trusted_publishing/github_configs"
    status, data = request_json(path + "?crate=" + urllib.parse.quote(crate, safe=""), token)
    if status != 200 or not isinstance(data.get("github_configs"), list):
        raise RuntimeError("Cannot retrieve authenticated crate publisher configurations")
    existing = data["github_configs"]
    if any(matches(config, expected) for config in existing):
        print("Verified existing matching OIDC publisher")
        return
    if any(config.get("repository_owner") == expected["repository_owner"]
           and config.get("repository_name") == expected["repository_name"]
           and config.get("workflow_filename") == WORKFLOW for config in existing):
        raise RuntimeError("Workflow already registered with a different environment")
    status, created = request_json(path, token, {"github_config": expected})
    if status != 200 or not matches(created.get("github_config", {}), expected):
        raise RuntimeError("Registry did not confirm the exact Trusted Publisher")
    print("Registered matching OIDC publisher")


def main():
    token = os.getenv("CARGO_REGISTRY_TOKEN")
    repo = os.getenv("GITHUB_REPOSITORY", "")
    ref = os.getenv("GITHUB_REF", "")
    workflow_ref = os.getenv("GITHUB_WORKFLOW_REF", "")
    if not token:
        raise RuntimeError("Missing protected bootstrap registry token")
    if ref != "refs/heads/main" or not workflow_ref.startswith(repo + "/.github/workflows/" + WORKFLOW + "@"):
        raise RuntimeError("Only main-branch release-rust.yml can register a publisher")
    crate, version = package_identity()
    assert_published(crate, version)
    ensure_publisher(crate, token, repo)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print("::error::Trusted Publisher registration failed: %s" % exc, file=sys.stderr)
        sys.exit(1)
