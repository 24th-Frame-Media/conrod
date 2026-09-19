"""Training the sharpness measure on ratings given by hand.

The Train screen asks how sharp the car in a crop is, the answers fit a small
model, and the model takes over the sharpness score. What matters:

  * what is stored against a rating is exactly what a scan would have
    measured, because it comes from the same call;
  * the model is a drop-in for the score, so the star bands, the verdict
    thresholds and the cull all keep meaning what they meant;
  * a pan is still decided by the hand-built measure, because whether the
    background is smeared is a fact and not a matter of taste;
  * with too few ratings, or ratings that do not beat the measure, nothing
    changes.

Synthetic crops again: a blur ladder is a property of the picture the measure
claims to read, where one real photograph would prove one photograph.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

import numpy as np
from PIL import Image, ImageFilter

from conrod import server, sharp_model, sharpness, store

BOX = (160, 120, 480, 360)

# Blur radius -> the rating a person would plausibly give the subject.
LADDER = [(0.0, 5), (0.7, 4), (1.4, 3), (2.2, 2), (3.5, 1), (6.0, 1)]


def _crop(radius: float, seed: int) -> Image.Image:
    """Detail as a lens records it -- band-limited noise -- then blurred."""
    rng = np.random.default_rng(seed)
    data = rng.integers(30, 225, size=(480, 640), dtype=np.uint8)
    base = Image.fromarray(data, mode="L").filter(ImageFilter.GaussianBlur(1.0))
    if radius:
        base = base.filter(ImageFilter.GaussianBlur(radius))
    return base.convert("RGB")


def _rated(n: int = 90):
    vectors, stars = [], []
    for i in range(n):
        radius, rating = LADDER[i % len(LADDER)]
        result = sharpness.measure(_crop(radius, seed=i), box=BOX)
        vectors.append(result.features)
        stars.append(rating)
    return vectors, stars


class Features(unittest.TestCase):
    def test_the_vector_is_as_long_as_it_is_named(self) -> None:
        result = sharpness.measure(_crop(0.0, 1), box=BOX)
        self.assertEqual(len(result.features), len(sharpness.FEATURE_NAMES))
        self.assertTrue(all(np.isfinite(result.features)))

    def test_the_score_is_the_first_feature(self) -> None:
        """train() reads the hand-built score off the row rather than
        re-measuring, and that only works while this holds."""
        with mock.patch.object(sharp_model, "current", return_value=None):
            result = sharpness.measure(_crop(1.4, 2), box=BOX)
        self.assertAlmostEqual(result.features[0], result.score)

    def test_no_subject_no_features(self) -> None:
        """Nothing measurable, nothing to learn from -- and no crash."""
        flat = Image.new("RGB", (640, 480), (128, 128, 128))
        self.assertEqual(sharpness.measure(flat, box=BOX).features, ())


class TheModel(unittest.TestCase):
    def test_too_few_ratings_is_no_model(self) -> None:
        self.assertIsNone(sharp_model.fit([[0.0] * 18] * 10, [1, 2, 3, 4, 5] * 2))

    def test_all_one_rating_is_no_model(self) -> None:
        self.assertIsNone(sharp_model.fit([[float(i)] * 18 for i in range(80)],
                                          [3] * 80))

    def test_a_model_from_another_feature_version_is_ignored(self) -> None:
        model = {"version": sharp_model.FEATURE_VERSION + 1, "mean": [0.0],
                 "spread": [1.0], "weights": [1.0], "intercept": 0.0}
        self.assertIsNone(sharp_model.predict(model, [1.0]))

    def test_a_vector_of_the_wrong_length_is_not_guessed_at(self) -> None:
        model = {"version": sharp_model.FEATURE_VERSION, "mean": [0.0, 0.0],
                 "spread": [1.0, 1.0], "weights": [1.0, 1.0], "intercept": 0.0}
        self.assertIsNone(sharp_model.predict(model, [1.0]))

    def test_it_learns_a_blur_ladder(self) -> None:
        vectors, stars = _rated()
        baseline = [sharpness.stars_for(v[0]) for v in vectors]
        result = sharp_model.agreement(vectors, stars, baseline)
        self.assertGreaterEqual(result["model"]["within_one"], 0.95)
        self.assertLessEqual(result["model"]["mean_error"],
                             result["measure"]["mean_error"])

    def test_predictions_fall_as_blur_rises(self) -> None:
        vectors, stars = _rated()
        model = sharp_model.fit(vectors, stars)
        by_radius = {}
        for i, (radius, _) in enumerate(LADDER):
            fresh = sharpness.measure(_crop(radius, seed=500 + i), box=BOX)
            by_radius[radius] = sharp_model.predict(model, fresh.features)
        ordered = [by_radius[r] for r, _ in LADDER]
        self.assertEqual(ordered, sorted(ordered, reverse=True))


class ADropInScore(unittest.TestCase):
    def _with_prediction(self, stars: float) -> float | None:
        flat = {"version": sharp_model.FEATURE_VERSION,
                "mean": [0.0] * len(sharpness.FEATURE_NAMES),
                "spread": [1.0] * len(sharpness.FEATURE_NAMES),
                "weights": [0.0] * len(sharpness.FEATURE_NAMES),
                "intercept": stars}
        with mock.patch.object(sharp_model, "current", return_value=flat):
            return sharpness._learned_score((0.0,) * len(sharpness.FEATURE_NAMES))

    def test_the_star_bands_give_back_the_rounded_prediction(self) -> None:
        """The reason the mapping has knots on the band floors. Predicting 3.4
        has to land in the three-star band, or every threshold in the cull
        would quietly mean something else once a model existed."""
        for stars in (1.0, 1.4, 1.6, 2.4, 2.6, 3.4, 3.6, 4.4, 4.6, 5.0, 5.4):
            score = self._with_prediction(stars)
            self.assertEqual(sharpness.stars_for(score),
                             min(5, max(1, round(stars))), stars)

    def test_the_worst_frames_stay_in_order(self) -> None:
        """Below one star the scale must not go flat: sorting worst-first is
        what the low end is for."""
        scores = [self._with_prediction(s) for s in (-0.5, 0.0, 0.4, 0.9)]
        self.assertEqual(scores, sorted(scores))
        self.assertLess(scores[0], scores[-1])

    def test_a_pan_is_still_the_hand_built_measures_call(self) -> None:
        pan = _crop(0.0, 3)
        smear = _crop(6.0, 4).crop((0, 0, 640, 480))
        frame = smear.copy()
        frame.paste(pan.crop(BOX), BOX[:2])
        with mock.patch.object(sharp_model, "current", return_value=None):
            plain = sharpness.measure(frame, box=BOX)
        pessimist = {"version": sharp_model.FEATURE_VERSION,
                     "mean": [0.0] * len(sharpness.FEATURE_NAMES),
                     "spread": [1.0] * len(sharpness.FEATURE_NAMES),
                     "weights": [0.0] * len(sharpness.FEATURE_NAMES),
                     "intercept": 1.0}
        with mock.patch.object(sharp_model, "current", return_value=pessimist):
            learned = sharpness.measure(frame, box=BOX)
        self.assertTrue(plain.panning)
        self.assertEqual(learned.panning, plain.panning)
        self.assertTrue(learned.learned)
        self.assertAlmostEqual(learned.heuristic, plain.score)


class TheStore(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = TemporaryDirectory()
        self.conn = store.connect(Path(self.tmp.name) / "t.db")
        job = self.conn.execute(
            "INSERT INTO jobs (root, created_at) VALUES ('x', 0)").lastrowid
        image = self.conn.execute(
            "INSERT INTO images (job_id, path) VALUES (?, 'a.cr3')", (job,)).lastrowid
        self.conn.execute(
            """INSERT INTO detections (image_id, x1, y1, x2, y2, crop_path,
                                       sharpness, panning)
               VALUES (?, 1.5, 2.5, 30.25, 40.75, 'c.jpg', 0.42, 0)""", (image,))

    def tearDown(self) -> None:
        self.conn.close()
        self.tmp.cleanup()

    def _next(self):
        return store.next_to_rate(self.conn, pan=0, low=0.4, high=0.5)

    def test_a_rated_crop_is_not_offered_again_and_undo_brings_it_back(self) -> None:
        self.assertIsNotNone(self._next())
        store.add_sharpness_label(
            self.conn, "a.cr3", (1.5, 2.5, 30.25, 40.75), stars=3, pan=False,
            heur_pan=False, features=(1.0, 2.0), version=1)
        self.assertIsNone(self._next())
        store.undo_sharpness_label(self.conn)
        self.assertIsNotNone(self._next())

    def test_only_this_versions_ratings_are_learned_from(self) -> None:
        for stars, version in ((3, 1), (4, 2)):
            store.add_sharpness_label(
                self.conn, f"f{version}.cr3", (0, 0, 9, 9), stars=stars, pan=False,
                heur_pan=False, features=(1.0,), version=version)
        store.add_sharpness_label(
            self.conn, "cant.cr3", (0, 0, 9, 9), stars=0, pan=False,
            heur_pan=False, features=(), version=1)
        rows = store.sharpness_labels(self.conn, 1)
        self.assertEqual([r["stars"] for r in rows], [3])

    def test_a_slice_outside_the_range_finds_nothing(self) -> None:
        self.assertIsNone(store.next_to_rate(self.conn, pan=0, low=0.5, high=0.6))
        self.assertIsNone(store.next_to_rate(self.conn, pan=1, low=0.4, high=0.5))


class EndToEnd(unittest.TestCase):
    """Rate crops through the real endpoints, learn, and see it take effect."""

    def setUp(self) -> None:
        from fastapi.testclient import TestClient

        self.tmp = TemporaryDirectory()
        self.client = TestClient(server.app)
        self.model_dir = mock.patch.object(sharp_model, "MODEL_DIR", Path(self.tmp.name))
        self.model_dir.start()
        sharp_model._cache["stamp"] = -1
        self.conn = store.connect()
        self.job = self.conn.execute(
            "INSERT INTO jobs (root, created_at) VALUES ('train-test', 0)").lastrowid
        self.dets = []
        for i in range(90):
            radius, rating = LADDER[i % len(LADDER)]
            crop = Path(self.tmp.name) / f"{i}.jpg"
            _crop(radius, seed=i).save(crop, "JPEG", quality=95)
            image = self.conn.execute(
                "INSERT INTO images (job_id, path) VALUES (?, ?)",
                (self.job, f"train-{i}.cr3")).lastrowid
            det = self.conn.execute(
                """INSERT INTO detections (image_id, x1, y1, x2, y2, crop_path,
                                           sharpness, panning)
                   VALUES (?, ?, ?, ?, ?, ?, ?, 0)""",
                (image, *BOX, str(crop), 0.05 + i / 100.0)).lastrowid
            self.dets.append((det, rating))

    def tearDown(self) -> None:
        self.conn.execute("DELETE FROM jobs WHERE id=?", (self.job,))
        self.conn.execute("DELETE FROM sharpness_labels WHERE path LIKE 'train-%'")
        self.conn.close()
        self.model_dir.stop()
        sharp_model._cache["stamp"] = -1
        self.tmp.cleanup()

    def _rate_all(self) -> None:
        for det, rating in self.dets:
            r = self.client.post("/api/training/label",
                                 json={"det": det, "stars": rating, "pan": False})
            self.assertEqual(r.status_code, 200, r.text)
            self.assertTrue(r.json()["stored"])

    def test_it_will_not_learn_from_too_little(self) -> None:
        det, rating = self.dets[0]
        self.client.post("/api/training/label",
                         json={"det": det, "stars": rating, "pan": False})
        self.assertEqual(self.client.post("/api/training/train").status_code, 400)

    def test_next_offers_a_crop_with_the_cars_box(self) -> None:
        got = self.client.get(f"/api/training/next?job={self.job}").json()
        self.assertIn(got["det"], [d for d, _ in self.dets])
        self.assertEqual(len(got["box"]), 4)
        self.assertTrue(all(0.0 <= v <= 1.0 for v in got["box"]), got["box"])

    def test_rating_then_learning_switches_the_score_over(self) -> None:
        self._rate_all()
        state = self.client.get("/api/training").json()
        self.assertEqual(state["rated"], 90)
        self.assertIsNone(state["model"])

        trained = self.client.post("/api/training/train").json()
        self.assertTrue(trained["active"], trained)
        self.assertGreaterEqual(trained["model"]["within_one"], 0.95)
        self.assertEqual(self.client.get("/api/training").json()["model"]["trained_on"],
                         90)

        sharp = sharpness.measure(_crop(0.0, 900), box=BOX)
        blurred = sharpness.measure(_crop(6.0, 901), box=BOX)
        self.assertTrue(sharp.learned)
        self.assertGreaterEqual(sharpness.stars_for(sharp.score), 4)
        self.assertLessEqual(sharpness.stars_for(blurred.score), 2)

        forgotten = self.client.delete("/api/training/model").json()
        self.assertIsNone(forgotten["model"])
        self.assertFalse(sharpness.measure(_crop(0.0, 900), box=BOX).learned)

    def test_cant_tell_is_remembered_but_never_learned_from(self) -> None:
        det, _ = self.dets[0]
        self.client.post("/api/training/label", json={"det": det, "stars": 0})
        state = self.client.get("/api/training").json()
        self.assertEqual((state["rated"], state["unsure"]), (0, 1))
        self.assertNotEqual(self.client.get(f"/api/training/next?job={self.job}"
                                            ).json()["det"], det)


class TheScreen(unittest.TestCase):
    def test_it_is_reachable(self) -> None:
        web = Path(server.__file__).parent / "web"
        markup = (web / "index.html").read_text(encoding="utf-8")
        code = (web / "app.js").read_text(encoding="utf-8")
        for want in ('data-screen="train"', "screen-train", "train-fit"):
            self.assertIn(want, markup)
        for want in ("/api/training/label", "/api/training/train", 'screen === "train"'):
            self.assertIn(want, code)


if __name__ == "__main__":
    unittest.main()
