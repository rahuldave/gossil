"""Count the packages that the six gix crates of the Git reader add to the build on this host.

The script runs `cargo tree` twice over the normal and build edges of the host target: once in
full, and once with the six gix crates pruned. It prints the packages that only the first run
has, and their count. It changes no file.
"""

import subprocess
import sys

CRATES = ["gix-discover", "gix-config", "gix-ref", "gix-odb", "gix-object", "gix-hash"]


def packages(prune):
    """Return the set of `name version` pairs that `cargo tree` prints, with `prune` left out."""
    command = ["cargo", "tree", "--locked", "--offline", "-e", "normal,build", "--prefix", "none"]
    command += ["--format", "{p}"]
    for crate in prune:
        command += ["--prune", crate]
    output = subprocess.run(command, check=True, capture_output=True, text=True).stdout
    return {" ".join(line.split()[:2]) for line in output.splitlines() if line.strip()}


def main(packages=packages):
    """Print the added packages and their count, and fail when none are found.

    `packages` is the function that lists the packages of a `cargo tree` run; a test passes a stub.
    """
    full = packages([])
    without = packages(CRATES)
    added = sorted(full - without)
    for package in added:
        print(package)
    print(f"added packages: {len(added)} of {len(full)} on the host target")
    return 0 if added else 1


if __name__ == "__main__":
    sys.exit(main())
