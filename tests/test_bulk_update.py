"""The bulk buttons in Review: reject several crops, or give them one number.

/api/detections/{det_id} was declared ahead of /api/detections/bulk and
matched it first, so both buttons answered 422 "not a valid integer" from the
day they were added. Nothing exercised the endpoint, which is how that went
unnoticed.
"""

from __future__ import annotations

import json
import unittest

from fastapi.testclient import TestClient

from conrod import server, store


class BulkUpdate(unittest.TestCase):
    def setUp(self) -> None:
        self.client = TestClient(server.app)
        self.conn = store.connect()
        self.job = self.conn.execute(
            "INSERT INTO jobs (root, created_at) VALUES ('bulk-test', 0)").lastrowid
        self.dets = []
        for i in range(3):
            image = self.conn.execute(
                "INSERT INTO images (job_id, path) VALUES (?, ?)",
                (self.job, f"bulk-{i}.cr3")).lastrowid
            self.dets.append(self.conn.execute(
                """INSERT INTO detections (image_id, x1, y1, x2, y2, attributes)
                   VALUES (?, 0, 0, 10, 10, ?)""",
                (image, json.dumps({}))).lastrowid)

    def tearDown(self) -> None:
        self.conn.execute("DELETE FROM jobs WHERE id=?", (self.job,))
        self.conn.close()

    def _row(self, det: int):
        return self.conn.execute(
            "SELECT number, rejected, reviewed FROM detections WHERE id=?",
            (det,)).fetchone()

    def test_rejecting_several_at_once(self) -> None:
        response = self.client.post("/api/detections/bulk",
                                    json={"ids": self.dets[:2], "rejected": True})
        self.assertEqual(response.status_code, 200, response.text)
        self.assertEqual(response.json()["updated"], 2)
        self.assertEqual([self._row(d)["rejected"] for d in self.dets], [1, 1, 0])

    def test_giving_several_one_number(self) -> None:
        response = self.client.post("/api/detections/bulk",
                                    json={"ids": self.dets, "number": "#88"})
        self.assertEqual(response.status_code, 200, response.text)
        self.assertEqual({self._row(d)["number"] for d in self.dets}, {"88"})

    def test_a_single_detection_still_routes_to_its_own_endpoint(self) -> None:
        """The fix must not move the problem: the numeric route has to keep
        answering for real ids."""
        response = self.client.post(f"/api/detections/{self.dets[0]}",
                                    json={"number": "7"})
        self.assertEqual(response.status_code, 200, response.text)
        self.assertEqual(self._row(self.dets[0])["number"], "7")


if __name__ == "__main__":
    unittest.main()
