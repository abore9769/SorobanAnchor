"""Regression tests for API snapshot diff exit statuses."""

import json
import subprocess
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).parents[1] / "scripts" / "diff_api_snapshot.sh"


def run_snapshot_diff(old_items, new_items):
    with tempfile.TemporaryDirectory() as temp_dir:
        old_path = Path(temp_dir) / "old.json"
        new_path = Path(temp_dir) / "new.json"
        old_path.write_text(json.dumps(old_items))
        new_path.write_text(json.dumps(new_items))
        return subprocess.run(
            ["bash", str(SCRIPT), str(old_path), str(new_path)],
            capture_output=True,
            text=True,
            check=False,
        )


def test_removed_snapshot_item_returns_failure_and_reports_diff():
    result = run_snapshot_diff(
        [{"name": "kept"}, {"name": "removed"}],
        [{"name": "kept"}],
    )

    assert result.returncode == 1
    assert "removed" in result.stdout
    assert "RESULT: Breaking changes detected." in result.stderr


def test_matching_snapshot_names_return_success():
    result = run_snapshot_diff(
        [{"name": "one"}, {"name": "two"}],
        [{"name": "two"}, {"name": "one"}],
    )

    assert result.returncode == 0
    assert "No public API surface changes detected." in result.stdout
