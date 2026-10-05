"""Tests of the helper's progress bar against the real `huggingface_hub`.

Run from this directory, with an interpreter that has the packages of
`hf_xet_requirements.txt` installed:

    python -m unittest test_hf_xet_downloader

Nothing here touches the network. The bar is driven the way the Hub drives
it: through its own Xet reporter, and with the calls its HTTP download makes.
"""
import contextlib
import gc
import io
import json
import logging
import sys
import unittest
from types import SimpleNamespace
from unittest import mock

from huggingface_hub.utils._xet_progress_reporting import XetDownloadProgressReporter

import hf_xet_downloader
from hf_xet_downloader import JsonProgressBar


class Recorded:
    """What a bar wrote to stdout while the block ran."""

    def __init__(self) -> None:
        self._stdout = io.StringIO()

    @property
    def progress(self) -> list:
        lines = self._stdout.getvalue().splitlines()
        return [json.loads(line) for line in lines]

    @property
    def last(self) -> dict:
        return self.progress[-1]


@contextlib.contextmanager
def recording():
    """Capture the helper's JSON lines, and keep tqdm's own bar off the terminal."""
    recorded = Recorded()
    with contextlib.redirect_stdout(recorded._stdout):
        with contextlib.redirect_stderr(io.StringIO()):
            yield recorded


def xet_report(written: int, received: int, total: int) -> SimpleNamespace:
    """A progress report as `hf_xet` hands one to the Hub's reporter."""
    return SimpleNamespace(
        total_bytes=total,
        total_bytes_completed=written,
        total_bytes_completion_rate=None,
        total_transfer_bytes_completed=received,
        total_transfer_bytes_completion_rate=None,
    )


def xet_reporter(total: int) -> XetDownloadProgressReporter:
    return XetDownloadProgressReporter(
        reconstruction_desc="model.gguf: reconstructing file",
        total=total,
        log_level=logging.WARNING,
        tqdm_class=JsonProgressBar,
    )


class TheXetReporter(unittest.TestCase):
    def test_the_hub_builds_one_bar_for_this_class(self):
        with recording() as recorded:
            with xet_reporter(total=1000) as reporter:
                self.assertIs(reporter.transfer_bar, reporter.reconstruction_bar)
                reporter.update_progress(xet_report(written=100, received=700, total=1000))

        self.assertEqual(
            recorded.last,
            {"status": "progress", "written": 100, "received": 700, "total": 1000},
        )

    def test_every_line_carries_both_counts(self):
        with recording() as recorded:
            with xet_reporter(total=1000) as reporter:
                reporter.update_progress(xet_report(written=0, received=300, total=1000))
                reporter.update_progress(xet_report(written=1000, received=900, total=1000))

        for line in recorded.progress:
            self.assertEqual(set(line), {"status", "written", "received", "total"})

    def test_bytes_written_after_the_last_one_received_are_reported(self):
        # The end of a Xet download: everything has been received and the
        # file is still being put together.
        with recording() as recorded:
            with mock.patch.object(hf_xet_downloader, "MIN_PROGRESS_INTERVAL_S", 0.0):
                with xet_reporter(total=1000) as reporter:
                    reporter.update_progress(xet_report(written=100, received=900, total=1000))
                    reporter.update_progress(xet_report(written=350, received=900, total=1000))
                    before_close = recorded.last

        self.assertEqual(
            before_close,
            {"status": "progress", "written": 350, "received": 900, "total": 1000},
        )


class TheBar(unittest.TestCase):
    def test_a_resume_starts_received_at_zero(self):
        with recording() as recorded:
            bar = JsonProgressBar(total=1000, initial=400, unit="B", unit_scale=True)
            opening = recorded.progress
            bar.close()

        self.assertEqual(
            opening,
            [{"status": "progress", "written": 400, "received": 0, "total": 1000}],
        )

    def test_a_bar_of_unknown_size_says_nothing_until_it_moves(self):
        with recording() as recorded:
            bar = JsonProgressBar()
            opening = recorded.progress
            bar.close()

        self.assertEqual(opening, [])

    def test_a_disabled_bar_still_counts(self):
        # tqdm itself keeps no count for a disabled bar.
        with recording() as recorded:
            bar = JsonProgressBar(total=1000, disable=True)
            bar.update(250)
            bar.close()

        self.assertEqual(recorded.last["written"], 250)

    def test_a_bar_that_was_never_built_closes_quietly(self):
        # tqdm rejects an argument it does not know, and closes what there is
        # of the bar when it is collected.
        unraisable = []
        with recording() as recorded:
            with mock.patch.object(sys, "unraisablehook", unraisable.append):
                with self.assertRaises(Exception):
                    JsonProgressBar(total=1000, no_such_argument=1)
                gc.collect()

        self.assertEqual(unraisable, [])
        self.assertEqual(recorded.progress, [])

    def test_close_emits_once_inside_the_throttle(self):
        with recording() as recorded:
            bar = JsonProgressBar(total=1000)
            # Inside the throttle window of the line the constructor wrote,
            # so neither update is reported on its own.
            bar.update(600)
            bar.update_transfer(600)
            self.assertEqual(len(recorded.progress), 1)

            bar.close()
            bar.close()
            bar.__del__()

        self.assertEqual(len(recorded.progress), 2)
        self.assertEqual(recorded.last["written"], 600)
        self.assertEqual(recorded.last["received"], 600)

    def test_http_chunks_count_once(self):
        # What `http_get` does with each chunk it reads.
        with recording() as recorded:
            bar = JsonProgressBar(total=300)
            for _ in range(3):
                bar.update(100)
                bar.update_transfer(100)
            bar.close()

        self.assertEqual(recorded.last["written"], 300)
        self.assertEqual(recorded.last["received"], 300)

    def test_a_rollback_never_lowers_received(self):
        # What `http_get` does when a server answers a range request with the
        # whole file: it takes the resumed bytes back off the bar.
        with recording() as recorded:
            bar = JsonProgressBar(total=1000)
            bar.update(400)
            bar.update_transfer(400)
            bar.update(-400)
            bar.update_transfer(-400)
            bar.close()

        self.assertEqual(recorded.last["written"], 0)
        self.assertEqual(recorded.last["received"], 400)

    def test_an_unthrottled_update_is_reported_as_it_happens(self):
        with recording() as recorded:
            with mock.patch.object(hf_xet_downloader, "MIN_PROGRESS_INTERVAL_S", 0.0):
                bar = JsonProgressBar(total=1000)
                bar.update_transfer(250)

            self.assertEqual(recorded.last["received"], 250)
            bar.close()


if __name__ == "__main__":
    unittest.main()
