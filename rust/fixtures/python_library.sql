BEGIN TRANSACTION;
CREATE TABLE detections (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    image_id      INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    x1 REAL, y1 REAL, x2 REAL, y2 REAL,
    cls           TEXT,
    conf          REAL,
    crop_path     TEXT,
    number        TEXT,
    number_source TEXT,          -- 'ocr' | 'roundel' | 'vlm' | 'manual' | pairs
    number_conf   REAL,
    plate         TEXT,
    plate_state   TEXT,
    plate_conf    REAL,
    -- The full VehicleAnalysis as JSON: make, model, colour, team, sponsors,
    -- text. Kept as one blob because the fields are read and written together
    -- and the shape is still moving.
    attributes    TEXT,
    reviewed      INTEGER NOT NULL DEFAULT 0,
    rejected      INTEGER NOT NULL DEFAULT 0
, signature TEXT, group_key INTEGER, group_size INTEGER, group_agreement REAL, colour_hex TEXT, group_colour_hex TEXT, sharpness REAL, sharpness_verdict TEXT, cull_reason TEXT, clipped INTEGER, rating REAL, rating_verdict TEXT, stars INTEGER, bystander INTEGER, panning INTEGER, background REAL, sharp_end TEXT, uncertain INTEGER, embedding TEXT, predicted_stars INTEGER, burst_pick INTEGER);
INSERT INTO "detections" VALUES(1,1,1.0,2.0,3.0,4.0,'car',0.8,'C:/shoot2/crops/1.jpg','7','ocr',0.95,NULL,NULL,NULL,NULL,0,0,NULL,NULL,NULL,NULL,NULL,NULL,0.6,'soft',NULL,1,2.5,'fair',NULL,NULL,1,0.2,'left',1,NULL,NULL,NULL);
INSERT INTO "detections" VALUES(2,2,0.0,0.0,1.0,1.0,'car',0.4,'C:/shoot2/crops/2.jpg',NULL,NULL,NULL,NULL,NULL,NULL,NULL,0,1,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,'no plate',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,1,NULL,NULL,NULL);
CREATE TABLE images (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id       INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    path         TEXT NOT NULL,
    preview_path TEXT,
    width        INTEGER,
    height       INTEGER,
    status       TEXT NOT NULL DEFAULT 'pending',
    error        TEXT,
    written_at   REAL, camera TEXT, burst_key INTEGER, taken_at REAL, rating_in_file INTEGER, label_in_file TEXT, sharpness REAL, rating REAL, rating_verdict TEXT,
    UNIQUE (job_id, path)
);
INSERT INTO "images" VALUES(1,1,'C:\shoot2\a.jpg',NULL,NULL,NULL,'pending',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "images" VALUES(2,1,'C:\shoot2\b.jpg',NULL,NULL,NULL,'pending',NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE TABLE jobs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    root          TEXT NOT NULL,
    label         TEXT,
    created_at    REAL NOT NULL,
    status        TEXT NOT NULL DEFAULT 'scanning',
    settings_json TEXT
);
INSERT INTO "jobs" VALUES(1,'C:\shoot2','Py Shoot',1.789896215490959167e+09,'scanning','{"b": 2}');
CREATE TABLE known_vehicles (
    plate       TEXT PRIMARY KEY,
    make        TEXT,
    model       TEXT,
    colour      TEXT,
    body_type   TEXT,
    team        TEXT,
    sponsors    TEXT,          -- comma separated, as a CSV column would be
    race_number TEXT,
    -- Other spellings of this plate seen on the same car, comma separated.
    -- Not guesses: grouping joins 43111J to 73111J only on visual evidence
    -- inside one burst, so these are readings of a plate this car was
    -- actually wearing. That makes a lookup on a misread safe here in a way
    -- that fuzzy-matching an arbitrary plate never is.
    aliases     TEXT,
    updated_at  REAL
);
CREATE TABLE sharpness_labels (
    path            TEXT NOT NULL,
    x1 REAL NOT NULL, y1 REAL NOT NULL, x2 REAL NOT NULL, y2 REAL NOT NULL,
    stars           INTEGER NOT NULL,   -- 1..5 of the subject, 0 = cannot tell
    pan             INTEGER NOT NULL DEFAULT 0,
    heur_pan        INTEGER,            -- what the measure said, for scoring it
    features        TEXT,
    feature_version INTEGER,
    created_at      REAL NOT NULL,
    PRIMARY KEY (path, x1, y1, x2, y2)
);
INSERT INTO "sharpness_labels" VALUES('C:\shoot2\a.jpg',1.0,2.0,3.0,4.0,5,0,0,'[0.3, 0.4]',1,1.789896215491895438e+09);
CREATE INDEX idx_images_job    ON images(job_id, status);
CREATE INDEX idx_det_image     ON detections(image_id);
CREATE INDEX idx_det_number    ON detections(number);
DELETE FROM "sqlite_sequence";
INSERT INTO "sqlite_sequence" VALUES('jobs',1);
INSERT INTO "sqlite_sequence" VALUES('images',2);
INSERT INTO "sqlite_sequence" VALUES('detections',2);
COMMIT;
