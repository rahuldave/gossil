#!/usr/bin/env python3
"""Supply the real pinned Linux reference tools behind generic project setup."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "evidence/reference-tools.json"
FOSSIL_FLAGS = ["--with-openssl=none", "--disable-fusefs"]
MAX_ARCHIVE = 32 * 1024 * 1024
MAX_SOURCE = 128 * 1024 * 1024


def digest(
    path: Path,  # File whose complete bytes identify an input or installed tool.
) -> str:  # SHA256 in hexadecimal.
    """Hash a file without retaining the complete file in memory."""
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def archive(
    pin: dict[str, str],  # Reviewed source identity and archive URL.
    cache: Path,  # Archive cache, which may already be populated read-only.
) -> Path:  # Verified archive path.
    """Reject changed cached bytes and verify downloads before exposing them."""
    target = cache / pin["archive"]
    if not target.exists():
        cache.mkdir(parents=True, exist_ok=True)
        with urlopen(pin["url"], timeout=30) as response:
            data = response.read(MAX_ARCHIVE + 1)
        if (
            len(data) > MAX_ARCHIVE
            or hashlib.sha256(data).hexdigest() != pin["sha256"]
        ):
            raise ValueError("reference archive size or SHA256 mismatch")
        with target.open("xb") as output:
            output.write(data)
    if digest(target) != pin["sha256"]:
        raise ValueError(f"reference archive SHA256 mismatch: {target}")
    return target


def unpack(
    source: Path,  # Previously verified source archive.
    destination: Path,  # Fresh task-owned extraction directory.
    root: str,  # The sole permitted archive root.
) -> Path:  # Extracted source root.
    """Extract bounded regular files, refusing traversal and all archive links."""
    with tarfile.open(source, "r:gz") as package:
        members = package.getmembers()
        if (
            len(members) > 10000
            or sum(item.size for item in members) > MAX_SOURCE
        ):
            raise ValueError("reference source exceeds extraction bounds")
        for item in members:
            path = PurePosixPath(item.name)
            if (
                path.is_absolute()
                or ".." in path.parts
                or path.parts[0] != root
            ):
                raise ValueError("reference archive path escapes its root")
            if not (item.isdir() or item.isfile()):
                raise ValueError(
                    "reference archive contains a link or special file"
                )
        for item in members:
            target = destination.joinpath(*PurePosixPath(item.name).parts)
            if item.isdir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with (
                    package.extractfile(item) as incoming,
                    target.open("xb") as output,
                ):
                    shutil.copyfileobj(incoming, output)
                target.chmod(item.mode & 0o777)
    return destination / root


def observe(
    tcl: Path,  # Real Tcl shell, including its installed runtime library.
    fossil: Path,  # Real pinned Fossil executable.
    pins: dict[str, dict[str, str]],  # Required versions and source revision.
    environment: dict[str, str]
    | None = None,  # Staged-library paths before publication.
) -> dict[str, str]:  # Observed tool output preserved in the install receipt.
    """Require the pinned versions, Tcl runtime and omitted TLS/FUSE features."""
    script = "puts [info patchlevel]\nputs [info library]\nputs [file exists [file join [info library] init.tcl]]\n"
    tcl_output = subprocess.run(
        [str(tcl)],
        input=script,
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
        env=environment,
    ).stdout
    lines = tcl_output.splitlines()
    if (
        len(lines) != 3
        or lines[0] != pins["tcl"]["version"]
        or lines[2] != "1"
    ):
        raise ValueError("Tcl version or installed runtime library mismatch")
    fossil_output = subprocess.run(
        [str(fossil), "version", "-v"],
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
        env=environment,
    ).stdout
    expected = f"This is fossil version {pins['fossil']['version']} [{pins['fossil']['uuid'][:10]}]"
    if not fossil_output.startswith(expected):
        raise ValueError("Fossil version or source revision mismatch")
    if any(
        line.startswith(("SSL (", "libfuse "))
        for line in fossil_output.splitlines()
    ):
        raise ValueError("Fossil reference unexpectedly enables TLS or FUSE")
    return {"tcl": tcl_output, "fossil": fossil_output}


def run(
    command: list[str],  # Native build command with literal arguments.
    cwd: Path,  # Task-owned source directory.
    environment: dict[str, str],  # Build environment with explicit Tcl shell.
) -> None:  # Errors retain the failed command and nonzero status.
    """Run one finite native build stage without a shell."""
    print("reference-tools:", command, flush=True)
    subprocess.run(command, cwd=cwd, env=environment, check=True, timeout=600)


def managed_observation(
    tools: dict[
        str, Path
    ],  # Resolved binaries in one existing managed prefix.
    pins: dict[str, dict[str, str]],  # Required source identities.
    lock_hash: str,  # Hash of the complete shipped source lock.
) -> dict[str, str]:  # Runtime observations after receipt and byte validation.
    """Check a managed installation read-only, including shared worktree tools."""
    prefix = tools["fossil"].parent
    if tools["tclsh"] != prefix / "tcl/bin/tclsh8.6":
        raise ValueError("reference tools do not share a managed prefix")
    receipt = json.loads((prefix / "receipt.json").read_text())
    if (
        receipt["source_lock_sha256"] != lock_hash
        or receipt["sources"] != pins
        or receipt["fossil_configure"] != FOSSIL_FLAGS
        or receipt["binaries"]
        != {name: digest(path) for name, path in tools.items()}
    ):
        raise ValueError("installed reference receipt or binary hash mismatch")
    return observe(tools["tclsh"], tools["fossil"], pins)


def setup(
    project: Path,  # Checkout receiving ignored .jlog tools and links.
    cache: Path
    | None,  # Optional archive cache for bounded offline verification.
) -> (
    None
):  # Validated real tools are published only after successful installation.
    """Build on Linux, keep existing non-Linux tools, and refuse foreign paths."""
    pins = json.loads(LOCK.read_text())
    bin_dir = project / ".jlog/bin"
    if sys.platform != "linux":
        print(
            json.dumps(
                {
                    "existing_tools_kept": observe(
                        bin_dir / "tclsh", bin_dir / "fossil", pins
                    )
                }
            )
        )
        return
    lock_hash = digest(LOCK)
    directories = [project / ".jlog", bin_dir, project / ".jlog/tools"]
    if any(path.is_symlink() for path in directories):
        shared = {
            name: (bin_dir / name).resolve() for name in ("tclsh", "fossil")
        }
        try:
            observed = managed_observation(shared, pins, lock_hash)
        except (OSError, ValueError, KeyError) as error:
            raise ValueError(
                "refusing unknown shared reference tools"
            ) from error
        print(json.dumps({"external_managed_tools_kept": observed}))
        return
    if cache is None:
        directories += [
            project / ".jlog/cache",
            project / ".jlog/cache/reference-tools",
        ]
    for path in directories:
        if path.is_symlink():
            raise ValueError(
                f"refusing shared reference-tool directory: {path}"
            )
    prefix = (
        project
        / ".jlog/tools"
        / f"reference-{lock_hash[:12]}-{platform.machine()}"
    )
    if prefix.is_symlink():
        raise ValueError("refusing a shared reference installation prefix")
    tools = {"tclsh": prefix / "tcl/bin/tclsh8.6", "fossil": prefix / "fossil"}
    for name, target in tools.items():
        link = bin_dir / name
        if (link.exists() or link.is_symlink()) and (
            not link.is_symlink() or link.resolve() != target.resolve()
        ):
            raise ValueError(
                f"refusing to replace existing reference tool: {link}"
            )
    receipt_path = prefix / "receipt.json"
    if prefix.exists():
        observed = managed_observation(tools, pins, lock_hash)
    else:
        cache = cache or project / ".jlog/cache/reference-tools"
        packages = {name: archive(pin, cache) for name, pin in pins.items()}
        prefix.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(
            prefix="reference-build-", dir=prefix.parent
        ) as temp:
            sources = {
                name: unpack(packages[name], Path(temp), pin["root"])
                for name, pin in pins.items()
            }
            for name, source in sources.items():
                if (source / "manifest.uuid").read_text().strip() != pins[
                    name
                ]["uuid"]:
                    raise ValueError(
                        "reference source manifest identity mismatch"
                    )
            if (
                hashlib.sha3_256(
                    (sources["fossil"] / "manifest").read_bytes()
                ).hexdigest()
                != pins["fossil"]["uuid"]
            ):
                raise ValueError(
                    "Fossil manifest bytes do not match the pinned revision"
                )
            environment = dict(os.environ)
            staging = Path(temp) / "installed"
            staged_prefix = staging / prefix.relative_to(prefix.anchor)
            staged_tools = {
                "tclsh": staged_prefix / "tcl/bin/tclsh8.6",
                "fossil": staged_prefix / "fossil",
            }
            tcl_build = sources["tcl"] / "unix"
            run(
                ["./configure", f"--prefix={prefix / 'tcl'}"],
                tcl_build,
                environment,
            )
            run(
                ["make", "-j1", "binaries", "libraries"],
                tcl_build,
                environment,
            )
            run(
                [
                    "make",
                    "install-binaries",
                    "install-libraries",
                    f"INSTALL_ROOT={staging}",
                ],
                tcl_build,
                environment,
            )
            environment.update(
                TCLSH=str(staged_tools["tclsh"]),
                LD_LIBRARY_PATH=str(staged_prefix / "tcl/lib"),
                TCL_LIBRARY=str(staged_prefix / "tcl/lib/tcl8.6"),
            )
            run(["./configure", *FOSSIL_FLAGS], sources["fossil"], environment)
            run(["make", "-j1", "fossil"], sources["fossil"], environment)
            shutil.copy2(sources["fossil"] / "fossil", staged_tools["fossil"])
            staged_observation = observe(
                staged_tools["tclsh"],
                staged_tools["fossil"],
                pins,
                environment,
            )
            (staged_prefix / "receipt.json").write_text(
                json.dumps(
                    {
                        "source_lock_sha256": lock_hash,
                        "sources": pins,
                        "fossil_configure": FOSSIL_FLAGS,
                        "jobs": 1,
                        "binaries": {
                            name: digest(path)
                            for name, path in staged_tools.items()
                        },
                        "staged_observation": staged_observation,
                    },
                    indent=2,
                )
                + "\n"
            )
            if prefix.exists() or prefix.is_symlink():
                raise ValueError(
                    "reference installation appeared during the build"
                )
            staged_prefix.rename(prefix)
        observed = observe(tools["tclsh"], tools["fossil"], pins)
    bin_dir.mkdir(parents=True, exist_ok=True)
    for name, target in tools.items():
        link = bin_dir / name
        if not link.is_symlink():
            link.symlink_to(os.path.relpath(target, bin_dir))
    print(json.dumps({"receipt": str(receipt_path), "observed": observed}))


def main() -> None:
    """Run the private provider used by generic setup and bounded Linux checks."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=ROOT)
    parser.add_argument("--archive-cache", type=Path)
    args = parser.parse_args()
    setup(args.project_root.resolve(), args.archive_cache)


if __name__ == "__main__":
    main()
