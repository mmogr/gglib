#!/usr/bin/env python3
"""Auxiliary downloader that uses `huggingface_hub` + `hf_xet` for fast transfers.

The script is intentionally lightweight: Rust invokes it with a download plan and
receives newline-delimited JSON events describing progress. The helper keeps all
stdout structured so the parent process can parse it deterministically.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, List, Optional

try:  # Import once so we can exit early if dependency resolution failed.
    from huggingface_hub import hf_hub_download  # type: ignore
except Exception as exc:  # pragma: no cover - surfaced to Rust caller.
    sys.stderr.write(
        f"huggingface_hub is required but could not be imported: {exc}\n"
    )
    sys.exit(97)

try:
    from tqdm.auto import tqdm  # type: ignore
except Exception as exc:  # pragma: no cover - surfaced to Rust caller.
    sys.stderr.write(f"tqdm is required but missing: {exc}\n")
    sys.exit(98)

try:
    import importlib.metadata as importlib_metadata
except ImportError:  # Python <3.8 fallback, not expected but harmless.
    import importlib_metadata  # type: ignore


MIN_PROGRESS_INTERVAL_S = 0.2
_LAST_PROGRESS_EMIT = 0.0


def record_progress_emit() -> None:
    global _LAST_PROGRESS_EMIT
    _LAST_PROGRESS_EMIT = time.monotonic()


def emit(status: str, **payload) -> None:
    """Emit a JSON protocol message with explicit status field.
    
    Protocol schema:
    - {"status": "progress", "file": "...", "downloaded": N, "total": N}
    - {"status": "unavailable", "reason": "..."}
    - {"status": "error", "message": "..."}
    - {"status": "complete"}
    """
    message = {"status": status, **payload}
    sys.stdout.write(json.dumps(message, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def require_hf_xet() -> str:
    try:
        import hf_xet  # type: ignore
    except Exception as exc:  # pragma: no cover - surfaced to Rust caller.
        emit(
            "unavailable",
            reason=f"hf_xet not available: {exc}",
        )
        sys.exit(90)

    try:
        return importlib_metadata.version("hf_xet")
    except importlib_metadata.PackageNotFoundError:
        return "unknown"


@dataclass
class FileSpec:
    path: str
    size: Optional[int]

    @property
    def display_name(self) -> str:
        return self.path



class JsonProgressBar(tqdm):
    """tqdm subclass that emits progress deltas as JSON."""

    def __init__(self, *args, **kwargs):
        # `huggingface_hub` sometimes forwards a `name` kwarg that vanilla tqdm
        # doesn't know about, so strip it here before delegating to super().
        kwargs.pop("name", None)
        desc = kwargs.get("desc") or ""
        super().__init__(*args, **kwargs)
        # tqdm 4.67 tightened attribute slots, so guard access to desc.
        self._label = getattr(self, "desc", None) or desc or ""
        self._last_emit = 0.0
        self._emit(force=True)

    def update(self, n=1):  # type: ignore[override]
        if getattr(self, "disable", False):
            # tqdm disables all accounting when `disable=True`, but we still
            # need to track byte counts for our JSON events even when the
            # helper is running in a non-interactive context.
            self.n += n
        else:
            super().update(n)
        self._emit()

    def _emit(self, force: bool = False) -> None:
        now = time.monotonic()
        if not force and now - self._last_emit < MIN_PROGRESS_INTERVAL_S:
            return
        emit(
            "progress",
            file=self._label,
            downloaded=int(self.n),
            total=int(self.total or 0),
        )
        self._last_emit = now
        record_progress_emit()


def parse_file_specs(raw_values: Iterable[str]) -> List[FileSpec]:
    specs: List[FileSpec] = []
    for raw in raw_values:
        raw = raw.strip()
        if not raw:
            continue
        size: Optional[int] = None
        path = raw
        for separator in ("::", "="):
            if separator in raw:
                candidate_path, candidate_size = raw.rsplit(separator, 1)
                path = candidate_path
                try:
                    size = int(candidate_size)
                except ValueError:
                    size = None
                break
        normalized_path = path.lstrip("/ ")
        if not normalized_path:
            raise ValueError(f"Invalid file specification: '{raw}'")
        specs.append(FileSpec(path=normalized_path, size=size))
    if not specs:
        raise ValueError("At least one --file argument is required")
    return specs


def ensure_dest_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def download_file(
    *,
    spec: FileSpec,
    args: argparse.Namespace,
    dest_root: Path,
    cache_dir: Optional[Path],
    hf_token: Optional[str],
) -> None:
    # Note: file-start is informational, not part of core protocol
    started = time.monotonic()
    destination_path = dest_root / spec.path
    ensure_dest_dir(destination_path.parent)

    downloaded_path = hf_hub_download(
        repo_id=args.repo_id,
        filename=spec.path,
        revision=args.revision,
        repo_type=args.repo_type,
        token=hf_token,
        cache_dir=cache_dir,
        local_dir=dest_root,
        force_download=args.force,
        local_files_only=args.local_only,
        tqdm_class=JsonProgressBar,
        resume_download=True,
    )

    if downloaded_path != destination_path:
        ensure_dest_dir(destination_path.parent)
        os.replace(downloaded_path, destination_path)

    finished = time.monotonic()
    # Note: file-complete is informational logging, not protocol
    duration_ms = int((finished - started) * 1000)
    sys.stderr.write(f"Downloaded {spec.display_name} in {duration_ms}ms\n")


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-id", required=True, help="<owner>/<repo>")
    parser.add_argument("--revision", default="main", help="Ref to download")
    parser.add_argument("--repo-type", default="model", help="Hub repo type")
    parser.add_argument("--dest", required=True, help="Directory for outputs")
    parser.add_argument("--cache-dir", help="Optional explicit cache directory")
    parser.add_argument("--token", help="Hub auth token")
    parser.add_argument(
        "--file",
        dest="files",
        action="append",
        required=True,
        help="File to fetch. Use '<path>::<size>' to hint size in bytes.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Force re-download even if cached",
    )
    parser.add_argument(
        "--local-only",
        action="store_true",
        help="Disable network access and rely on cache only",
    )
    parser.add_argument(
        "--probe",
        action="store_true",
        help="Only report readiness (used by Rust for health checks).",
    )
    return parser


def main() -> int:
    parser = build_arg_parser()
    args = parser.parse_args()

    os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
    os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")

    hf_xet_version = require_hf_xet()
    hub_version = importlib_metadata.version("huggingface_hub")

    if args.probe:
        emit(
            "probe",
            status="ok",
            huggingface_hub=hub_version,
            hf_xet=hf_xet_version,
        )
        return 0

    try:
        file_specs = parse_file_specs(args.files)
    except ValueError as exc:
        emit("error", message=str(exc))
        return 64

    dest_root = Path(args.dest).expanduser().resolve()
    cache_dir = Path(args.cache_dir).expanduser().resolve() if args.cache_dir else None
    hf_token = args.token or None

    # Log session info to stderr (not part of protocol)
    sys.stderr.write(
        f"Downloading from {args.repo_id}@{args.revision} to {dest_root}\n"
    )

    for spec in file_specs:
        try:
            download_file(
                spec=spec,
                args=args,
                dest_root=dest_root,
                cache_dir=cache_dir,
                hf_token=hf_token,
            )
        except Exception as exc:  # pragma: no cover - bubbled to Rust.
            emit("error", message=f"Failed to download {spec.display_name}: {exc}")
            return 65

    emit("complete")
    return 0


if __name__ == "__main__":
    sys.exit(main())
