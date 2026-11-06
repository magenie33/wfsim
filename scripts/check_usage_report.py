"""Usage reporting: calendar bounds, browser sets, shell separation and geography."""

import contextlib
import io
import json
import sys
import unittest
from datetime import date, datetime as real_datetime, timezone
from unittest.mock import patch

import usage


def point(day, cid="a", event="app.boot", shell="web", country="XX"):
    return {"day": day, "cid": cid, "e": event, "shell": shell, "country": country,
            "n": 1, "last_at": day + "T12:00:00Z"}


class UsageReport(unittest.TestCase):
    def report(self, rows, start="2026-10-23", before="2026-11-06"):
        return usage.visitor_report(rows, start, before, date(2026, 11, 6))

    def test_countries(self):
        self.assertEqual([usage.MARKET(c) for c in ("CN", "US", "GB", "XX", "", None, "ZZ", "T1", "us")],
                         ["china", "overseas", "overseas", *(["unknown"] * 6)])

    def test_period_and_shells(self):
        rows = [point("2026-11-04"), point("2026-11-05"), point("2026-11-05", event="simulator.run"),
                point("2026-11-05", shell="desktop"), point("2026-11-05", cid="b", event="engine.fail")]
        report = self.report(rows)
        self.assertEqual(report["period"]["all"], {"visitors": 2, "result": 1, "returning": 0})
        self.assertEqual(report["period"]["web"]["visitors"], 2)
        self.assertEqual(report["period"]["desktop"], {"visitors": 1, "result": 0, "returning": 0})
        self.assertEqual(report["daily"][-1]["shells"]["web"]["returning"], 1)
        self.assertEqual(report["daily"][-1]["shells"]["desktop"]["returning"], 0)
        self.assertFalse(report["partial"])

    def test_calendar_weeks_include_missing_dates(self):
        rows = [point("2026-10-23", "a"), point("2026-10-24", "a"),
                point("2026-10-30", "b"), point("2026-11-05", "a")]
        report = self.report(rows)
        self.assertEqual(len(report["daily"]), 14)
        self.assertEqual(report["weekly"]["web"], {"previous": 1, "current": 2, "returned": 1})
        self.assertEqual(report["daily"][2]["shells"]["web"]["visitors"], 0)

    def test_returning_boundary_and_history(self):
        rows = [point("2026-10-28", "old"), point("2026-10-29", "back"),
                point("2026-11-05", "old"), point("2026-11-05", "back")]
        report = self.report(rows, "2026-11-05")
        self.assertEqual(report["daily"][0]["shells"]["web"]["returning"], 1)
        self.assertTrue(report["returning_history_complete"])
        self.assertEqual(report["period"]["web"]["visitors"], 2)

    def test_json_has_no_browser_identifiers(self):
        cid = "a" * 32
        text = json.dumps(self.report([point("2026-11-05", cid)]))
        self.assertNotIn(cid, text)
        self.assertNotIn('"cid"', text)

    def test_complete_utc_days_and_shell_reach_every_query(self):
        class Clock:
            @classmethod
            def now(cls, tz):
                self.assertIs(tz, timezone.utc)
                return real_datetime(2026, 11, 6, 23, tzinfo=timezone.utc)

        with patch.object(usage, "datetime", Clock), patch.object(usage.Log, "q", return_value=[]) as query:
            out = io.StringIO()
            with patch.object(sys, "argv", ["usage.py", "--days", "7", "--shell", "web", "--json"]), contextlib.redirect_stdout(out):
                usage.main()
            report = json.loads(out.getvalue())
            self.assertEqual((report["from"], report["before"]), ("2026-10-30", "2026-11-06"))
            self.assertEqual(set(report["period"]), {"web"})
            self.assertEqual(query.call_count, 1)

        with patch.dict("os.environ", {"PG_URL": "unused"}):
            log = usage.Log("2026-10-30", "2026-11-06", "web")
            reply = '{"result":[{"results":[]}]}'
            with patch.object(usage.subprocess, "run") as run:
                run.return_value.stdout = reply
                for read in (log.rows, log.boots, log.probes, log.subjects, log.held):
                    read()
                    body = json.loads(run.call_args.kwargs["input"])
                    self.assertEqual(body["params"][:4], ["2026-10-30", "deploy-check", "2026-11-06", "web"])
                    self.assertIn("day < ?3", body["sql"])
                    self.assertIn("shell = ?4", body["sql"])

    def test_explicit_today_is_marked_partial(self):
        report = self.report([point("2026-11-06")], "2026-11-06", "2026-11-07")
        self.assertTrue(report["partial"])
        self.assertTrue(report["daily"][0]["partial"])


if __name__ == "__main__":
    with patch.dict("os.environ", {"PG_URL": "unused"}):
        unittest.main()
