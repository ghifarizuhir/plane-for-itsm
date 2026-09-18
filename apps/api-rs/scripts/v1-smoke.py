#!/usr/bin/env python3
"""Contract smoke: drive plane-sdk against the local Rust /api/v1.

Usage:
  V1_TOKEN=plane_api_... V1_WS=itsm /home/ghifari/plane-mcp-server/.venv/bin/python apps/api-rs/scripts/v1-smoke.py
Reads the SDK from plane-mcp-server/.venv (PLANE_SDK_PATH can override).
"""
import os
import sys

SDK = os.environ.get("PLANE_SDK_PATH", "/home/ghifari/plane-mcp-server/.venv/lib/python3.11/site-packages")
sys.path.insert(0, SDK)

from plane import PlaneClient  # noqa: E402

BASE = os.environ.get("V1_BASE", "http://localhost:8000")
TOKEN = os.environ["V1_TOKEN"]
WS = os.environ["V1_WS"]

client = PlaneClient(base_url=BASE, api_key=TOKEN)

lite = client.projects.list_lite(WS)
assert lite.total_count >= 0, "projects-lite missing envelope fields"
print(f"projects-lite OK: total_count={lite.total_count}")

if lite.results:
    pid = lite.results[0].id
    project = client.projects.retrieve(WS, pid)
    assert project.name and project.identifier, "retrieve missing required fields"
    print(f"project retrieve OK: {project.identifier} {project.name}")
    feats = client.projects.get_features(WS, pid)
    print(f"project features OK: work_item_types={feats.work_item_types}")
    wl = client.projects.get_worklog_summary(WS, pid)
    assert isinstance(wl, list), "worklog summary must be a list"
    print(f"worklog summary OK: {len(wl)} rows")

print("v1 smoke passed")
