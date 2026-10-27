"""Bounded Ubuntu prerequisites for 3D cargo-check/model tests, not GUI tests.

The locked ALSA and udev sys crates require pkg-config development libraries.
X11/EGL/Vulkan are dynamically loaded by the client, which this lane does not
launch. Ordinary native graphical lanes retain their separate prerequisites.
"""
import shlex
import subprocess


def commands():
    """Keep configured repositories/trust and bound apt's own acquisition retry."""
    options = ['-o', 'Acquire::Retries=2', '-o', 'Acquire::http::Timeout=20',
               '-o', 'Acquire::https::Timeout=20']
    # Downloads and dpkg configuration share this install budget. Slow but
    # progressing official mirrors can exhaust three minutes; stalled requests
    # still have the unchanged 20-second acquisition timeouts below.
    return [
        ['timeout', '--kill-after=10s', '2m', 'sudo', 'apt-get', *options,
         '-o', 'APT::Update::Error-Mode=any', 'update'],
        ['timeout', '--kill-after=10s', '10m', 'sudo', 'apt-get', *options,
         'install', '--no-install-recommends', '-y', 'pkg-config', 'libasound2-dev', 'libudev-dev'],
        ['timeout', '--kill-after=5s', '15s', 'pkg-config', '--print-errors', '--exists', 'alsa', 'libudev'],
    ]


def main():
    for command in commands():
        print('$ ' + shlex.join(command), flush=True)
        subprocess.run(command, check=True)


if __name__ == '__main__':
    main()
