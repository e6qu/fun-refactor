#!/usr/bin/env python3
"""Measure development SHA-256 throughput while checking identical digest bytes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def main():
    with tempfile.TemporaryDirectory(prefix='fr-hash-profile-', dir=os.environ['RUNNER_TEMP']) as directory:
        root = Path(directory)
        (root / 'src').mkdir()
        (root / 'Cargo.toml').write_text('[package]\nname="fr-hash-profile"\nversion="0.0.0"\nedition="2021"\n'
                                        '[dependencies]\nsha2="=0.11.0"\n')
        (root / 'src/main.rs').write_text('''use sha2::{Digest, Sha256};
fn main() {
    let data = vec![120u8; 4 * 1024 * 1024];
    let started = std::time::Instant::now();
    let mut digest = Sha256::new();
    for _ in 0..8 { digest.update(&data); }
    let nanos = started.elapsed().as_nanos();
    let digest: String = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    println!("{} {}", nanos, digest);
}
''')
        expected = hashlib.sha256(b'x' * (32 * 1024 * 1024)).hexdigest()
        timings = {}
        for level in (0, 3):
            result = subprocess.run(['cargo', 'run', '--offline', '--quiet', '--manifest-path', str(root / 'Cargo.toml'),
                                     '--config', f'profile.dev.package.sha2.opt-level={level}'],
                                    capture_output=True, text=True, check=True, timeout=60)
            nanos, digest = result.stdout.split()
            assert digest == expected, 'optimization must preserve the independent SHA-256 digest'
            timings[str(level)] = int(nanos) / 1_000_000_000
        print(json.dumps({'bytes': 32 * 1024 * 1024, 'sha256': expected, 'seconds_by_sha2_opt_level': timings,
                          'speedup': timings['0'] / timings['3']}, sort_keys=True), flush=True)


if __name__ == '__main__':
    main()
