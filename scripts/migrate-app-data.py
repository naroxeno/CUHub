#!/usr/bin/env python3
import argparse
import os
from pathlib import Path
import stat
import subprocess


def link_old_path(old, new):
    if os.name == "nt":
        env = dict(os.environ, CUPRUM_OLD_DATA=str(old), CUPRUM_NEW_DATA=str(new))
        subprocess.run(
            'cmd.exe /d /v:off /c mklink /J "%CUPRUM_OLD_DATA%" "%CUPRUM_NEW_DATA%"',
            env=env,
            check=True,
        )
    else:
        old.symlink_to(new.name, target_is_directory=True)


def migrate(old):
    source = old.lstat()
    if not stat.S_ISDIR(source.st_mode) or (
        getattr(source, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
    ):
        raise ValueError(f"Expected a real data directory: {old}")
    old = old.resolve()
    if not (old / "Settings.json").is_file():
        raise ValueError(f"Settings.json is missing from {old}")
    new = old.with_name("Cuprum")
    if os.path.lexists(new):
        raise ValueError(f"Destination already exists: {new}")
    old.rename(new)
    try:
        link_old_path(old, new)
    except (OSError, subprocess.CalledProcessError, KeyboardInterrupt):
        if not os.path.lexists(old):
            new.rename(old)
        raise
    return new


def main():
    parser = argparse.ArgumentParser(
        description="Move data to Cuprum. Close the app and game before running."
    )
    parser.add_argument("old_data_dir", type=Path)
    args = parser.parse_args()
    try:
        new = migrate(args.old_data_dir.expanduser())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Migration stopped: {error}\n")
    print(f"Data directory: {new}")
    print("Keep the link at the previous location so saved mod paths still resolve.")


if __name__ == "__main__":
    main()
