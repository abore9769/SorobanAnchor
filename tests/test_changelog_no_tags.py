"""Regression test for changelog generation in an untagged checkout."""

import os
import subprocess
import tempfile
from pathlib import Path


def test_dry_run_uses_cargo_version_when_checkout_has_no_tags():
    with tempfile.TemporaryDirectory() as temp_dir:
        root = Path(temp_dir)
        scripts_dir = root / "scripts"
        scripts_dir.mkdir()
        (root / "Cargo.toml").write_text('[package]\nversion = "1.2.3"\n')
        (root / "cliff.toml").write_text("")
        script = scripts_dir / "generate_changelog.sh"
        script.write_text(
            Path(__file__).parents[1]
            .joinpath("scripts/generate_changelog.sh")
            .read_text()
        )

        subprocess.run(["git", "init", "--quiet", str(root)], check=True)

        bin_dir = root / "bin"
        bin_dir.mkdir()
        args_file = root / "git-cliff-args.txt"
        git_cliff = bin_dir / "git-cliff"
        git_cliff.write_text(
            "#!/usr/bin/env bash\n"
            'printf "%s\\n" "$@" > "$CLIFF_ARGS_FILE"\n'
            'printf "no-tag fixture output\\n"\n'
        )
        git_cliff.chmod(0o755)

        env = os.environ.copy()
        env["CLIFF_ARGS_FILE"] = str(args_file)
        env["PATH"] = f"{bin_dir}{os.pathsep}{env['PATH']}"
        result = subprocess.run(
            ["bash", str(script), "--dry-run"],
            capture_output=True,
            text=True,
            env=env,
            check=False,
        )

        assert result.returncode == 0, result.stderr
        assert "No version tags found" in result.stderr
        assert "fallback tag v1.2.3" in result.stderr
        assert "no-tag fixture output" in result.stdout
        assert args_file.read_text().splitlines() == [
            "--unreleased",
            "--tag",
            "v1.2.3",
        ]
        assert not (root / "CHANGELOG.md").exists()
