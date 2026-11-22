#!/usr/bin/env python3
"""Capture the pinned native queue's equal-priority order in an isolated home."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--fossil', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
fossil = args.fossil.resolve(strict=True)
argv = ['test-pqueue', '1/0', '1/1', '1/2', '1/3', '1/4']
with tempfile.TemporaryDirectory(prefix='gossil-native-queue-') as temporary:
    env = dict(os.environ, HOME=temporary, FOSSIL_HOME=temporary, LC_ALL='C', TZ='UTC')
    result = subprocess.run([str(fossil), *argv], cwd=temporary, env=env, capture_output=True, text=True, timeout=10)
    if result.returncode:
        raise RuntimeError(result.stderr)
    pops = [int(m.group(1)) for m in re.finditer(r'\.\.\. POP\s+"(\d+)"', result.stdout)]
    if len(pops) != 5:
        raise RuntimeError('native queue capture was incomplete')
    payload = dict(format=1, scope='queue algorithm only; no repository or storage admission', argv=argv,
                   binary_sha256=hashlib.sha256(fossil.read_bytes()).hexdigest(),
                   returncode=result.returncode, stdout=result.stdout, stderr=result.stderr, pops=pops)
    args.output.write_text(json.dumps(payload, indent=2)+'\n')
print(f'Captured isolated native queue order {pops} to {args.output}')
