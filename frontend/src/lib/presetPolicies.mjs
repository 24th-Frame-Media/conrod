// Generated from the Rust policy table (conrod-core profile.rs). Do not edit:
// UPDATE_PRESET_POLICIES=1 cargo test -p conrod-core presets_match_the_frontend_copy
export default {
  "portrait": {
    "preset": "portrait",
    "label": "Portraits",
    "profile": "portrait",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "genuine_smile",
          "label": "Genuine smile",
          "weight": 0.8
        },
        {
          "key": "laughter",
          "label": "Laughter",
          "weight": 0.8
        },
        {
          "key": "eye_contact",
          "label": "Eye contact",
          "weight": 0.6
        },
        {
          "key": "candid",
          "label": "Candid moment",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "portrait-group": {
    "preset": "portrait-group",
    "label": "Group photos",
    "profile": "portrait",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "everyone_looking",
          "label": "Everyone looking",
          "weight": 0.8
        },
        {
          "key": "genuine_smile",
          "label": "Genuine smile",
          "weight": 0.8
        },
        {
          "key": "laughter",
          "label": "Laughter",
          "weight": 0.8
        }
      ]
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "portrait-couple": {
    "preset": "portrait-couple",
    "label": "Couple photos",
    "profile": "portrait",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "embrace",
          "label": "Embrace",
          "weight": 0.7
        },
        {
          "key": "laughing_together",
          "label": "Laughing together",
          "weight": 0.8
        },
        {
          "key": "eye_contact",
          "label": "Eye contact",
          "weight": 0.6
        },
        {
          "key": "kiss",
          "label": "Kiss",
          "weight": 0.8
        }
      ]
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "portrait-pets": {
    "preset": "portrait-pets",
    "label": "Pets",
    "profile": "portrait",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "pet_eye_contact",
          "label": "Eye contact",
          "weight": 0.7
        },
        {
          "key": "playful_action",
          "label": "Playful action",
          "weight": 0.8
        },
        {
          "key": "happy_face",
          "label": "Happy face",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "motorsport": {
    "preset": "motorsport",
    "label": "Motorsport",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "tyre_smoke",
          "label": "Tyre smoke",
          "weight": 0.6
        },
        {
          "key": "flames",
          "label": "Flames",
          "weight": 0.7
        },
        {
          "key": "sparks",
          "label": "Sparks",
          "weight": 0.6
        },
        {
          "key": "kerb_strike",
          "label": "Kerb strike",
          "weight": 0.5
        },
        {
          "key": "wheel_lift",
          "label": "Wheel lift",
          "weight": 0.6
        },
        {
          "key": "close_racing",
          "label": "Close racing",
          "weight": 0.7
        },
        {
          "key": "contact",
          "label": "Contact",
          "weight": 0.9
        },
        {
          "key": "crash",
          "label": "Crash or spin",
          "weight": 1.0
        },
        {
          "key": "chequered_flag",
          "label": "Chequered flag",
          "weight": 0.7
        },
        {
          "key": "podium",
          "label": "Podium",
          "weight": 0.6
        }
      ]
    },
    "panForgiven": true,
    "blur": "critical",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "motorsport-burnouts": {
    "preset": "motorsport-burnouts",
    "label": "Burnouts",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "smoke_cloud",
          "label": "Smoke cloud",
          "weight": 0.7
        },
        {
          "key": "flames",
          "label": "Flames",
          "weight": 0.7
        },
        {
          "key": "tyre_burst",
          "label": "Tyre burst",
          "weight": 0.7
        },
        {
          "key": "sparks",
          "label": "Sparks",
          "weight": 0.6
        },
        {
          "key": "crowd_reaction",
          "label": "Crowd reaction",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": true,
    "blur": "lenient",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "motorsport-track": {
    "preset": "motorsport-track",
    "label": "Track days",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "tyre_smoke",
          "label": "Tyre smoke",
          "weight": 0.6
        },
        {
          "key": "sparks",
          "label": "Sparks",
          "weight": 0.6
        },
        {
          "key": "kerb_strike",
          "label": "Kerb strike",
          "weight": 0.5
        },
        {
          "key": "wheel_lift",
          "label": "Wheel lift",
          "weight": 0.6
        },
        {
          "key": "close_racing",
          "label": "Close racing",
          "weight": 0.7
        },
        {
          "key": "contact",
          "label": "Contact",
          "weight": 0.9
        },
        {
          "key": "crash",
          "label": "Crash or spin",
          "weight": 1.0
        },
        {
          "key": "pit_lane",
          "label": "Pit lane work",
          "weight": 0.4
        }
      ]
    },
    "panForgiven": true,
    "blur": "critical",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "motorsport-rally": {
    "preset": "motorsport-rally",
    "label": "Rallies",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "airborne",
          "label": "Airborne",
          "weight": 0.9
        },
        {
          "key": "dust",
          "label": "Dust",
          "weight": 0.6
        },
        {
          "key": "water_splash",
          "label": "Water splash",
          "weight": 0.7
        },
        {
          "key": "sideways",
          "label": "Sideways",
          "weight": 0.7
        },
        {
          "key": "wheel_lift",
          "label": "Wheel lift",
          "weight": 0.6
        },
        {
          "key": "crash",
          "label": "Crash or spin",
          "weight": 1.0
        },
        {
          "key": "crowd_reaction",
          "label": "Crowd reaction",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": true,
    "blur": "lenient",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "motorsport-meet": {
    "preset": "motorsport-meet",
    "label": "Car meets",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "owner_with_car",
          "label": "Owner with car",
          "weight": 0.5
        },
        {
          "key": "badge_detail",
          "label": "Badge detail",
          "weight": 0.4
        },
        {
          "key": "engine_bay",
          "label": "Engine bay",
          "weight": 0.4
        },
        {
          "key": "interior",
          "label": "Interior",
          "weight": 0.4
        },
        {
          "key": "wheel_detail",
          "label": "Wheel detail",
          "weight": 0.3
        }
      ]
    },
    "panForgiven": false,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "event": {
    "preset": "event",
    "label": "Events",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "speaker_gesture",
          "label": "Speaker gesture",
          "weight": 0.6
        },
        {
          "key": "crowd_reaction",
          "label": "Crowd reaction",
          "weight": 0.5
        },
        {
          "key": "presentation",
          "label": "Presentation",
          "weight": 0.5
        },
        {
          "key": "laughter",
          "label": "Laughter",
          "weight": 0.8
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "event-shows": {
    "preset": "event-shows",
    "label": "Shows",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "performer_peak",
          "label": "Performer at peak",
          "weight": 0.7
        },
        {
          "key": "crowd_reaction",
          "label": "Crowd reaction",
          "weight": 0.5
        },
        {
          "key": "animal_in_motion",
          "label": "Animal in motion",
          "weight": 0.6
        },
        {
          "key": "animal_portrait",
          "label": "Animal portrait",
          "weight": 0.6
        },
        {
          "key": "prize_moment",
          "label": "Prize moment",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": true,
    "blur": "lenient",
    "levelCheck": false,
    "darkOk": true,
    "deliverMost": false
  },
  "event-parties": {
    "preset": "event-parties",
    "label": "Parties",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "laughter",
          "label": "Laughter",
          "weight": 0.8
        },
        {
          "key": "toast",
          "label": "Toast",
          "weight": 0.6
        },
        {
          "key": "dancing",
          "label": "Dancing",
          "weight": 0.7
        },
        {
          "key": "hug",
          "label": "Hug",
          "weight": 0.6
        },
        {
          "key": "candid",
          "label": "Candid moment",
          "weight": 0.5
        },
        {
          "key": "cake_candles",
          "label": "Cake and candles",
          "weight": 0.7
        }
      ]
    },
    "panForgiven": false,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "mix": {
    "preset": "mix",
    "label": "Mixed",
    "profile": "mix",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "airborne",
          "label": "Airborne",
          "weight": 0.9
        },
        {
          "key": "tyre_smoke",
          "label": "Tyre smoke",
          "weight": 0.6
        },
        {
          "key": "flames",
          "label": "Flames",
          "weight": 0.7
        },
        {
          "key": "laughter",
          "label": "Laughter",
          "weight": 0.8
        },
        {
          "key": "celebration",
          "label": "Celebration",
          "weight": 0.7
        },
        {
          "key": "crowd_reaction",
          "label": "Crowd reaction",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "sport-field": {
    "preset": "sport-field",
    "label": "Field sports",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Jersey numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "people",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "ball_contact",
          "label": "Ball contact",
          "weight": 0.8
        },
        {
          "key": "goal_or_try",
          "label": "Goal or try",
          "weight": 1.0
        },
        {
          "key": "celebration",
          "label": "Celebration",
          "weight": 0.7
        },
        {
          "key": "tackle",
          "label": "Tackle",
          "weight": 0.8
        },
        {
          "key": "peak_jump",
          "label": "Peak of a jump",
          "weight": 0.8
        }
      ]
    },
    "panForgiven": false,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "sport-court": {
    "preset": "sport-court",
    "label": "Court sports",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Jersey numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "people",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "shot_release",
          "label": "Shot release",
          "weight": 0.8
        },
        {
          "key": "dunk_or_spike",
          "label": "Dunk or spike",
          "weight": 1.0
        },
        {
          "key": "celebration",
          "label": "Celebration",
          "weight": 0.7
        },
        {
          "key": "block",
          "label": "Block",
          "weight": 0.8
        },
        {
          "key": "peak_jump",
          "label": "Peak of a jump",
          "weight": 0.8
        }
      ]
    },
    "panForgiven": true,
    "blur": "strict",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "sport-rodeo": {
    "preset": "sport-rodeo",
    "label": "Rodeo",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "rider_airborne",
          "label": "Rider airborne",
          "weight": 0.9
        },
        {
          "key": "bucking_peak",
          "label": "Bucking peak",
          "weight": 1.0
        },
        {
          "key": "dismount_or_fall",
          "label": "Dismount or fall",
          "weight": 0.8
        },
        {
          "key": "clown_action",
          "label": "Rodeo clown",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": false,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "sport-equestrian": {
    "preset": "sport-equestrian",
    "label": "Equestrian",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "jump_apex",
          "label": "Jump apex",
          "weight": 1.0
        },
        {
          "key": "hooves_off_ground",
          "label": "All hooves off ground",
          "weight": 0.9
        },
        {
          "key": "landing",
          "label": "Landing",
          "weight": 0.6
        },
        {
          "key": "rider_focus",
          "label": "Rider focus",
          "weight": 0.4
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "sport-cycling": {
    "preset": "sport-cycling",
    "label": "Cycling",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Bicycles",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "airborne",
          "label": "Airborne",
          "weight": 0.9
        },
        {
          "key": "sprint_finish",
          "label": "Sprint finish",
          "weight": 1.0
        },
        {
          "key": "crash",
          "label": "Crash or spin",
          "weight": 1.0
        },
        {
          "key": "peloton",
          "label": "Peloton",
          "weight": 0.5
        },
        {
          "key": "climb_effort",
          "label": "Climbing effort",
          "weight": 0.5
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "sport-action": {
    "preset": "sport-action",
    "label": "Action sports",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "airborne",
          "label": "Airborne",
          "weight": 0.9
        },
        {
          "key": "trick_peak",
          "label": "Trick at its peak",
          "weight": 1.0
        },
        {
          "key": "spray_or_dust",
          "label": "Spray or dust",
          "weight": 0.6
        },
        {
          "key": "crash",
          "label": "Crash or spin",
          "weight": 1.0
        },
        {
          "key": "grab",
          "label": "Grab",
          "weight": 0.7
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "property-listing": {
    "preset": "property-listing",
    "label": "Property listing",
    "profile": "mix",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": true
      },
      "vocabulary": []
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": true,
    "darkOk": false,
    "deliverMost": false
  },
  "property-report": {
    "preset": "property-report",
    "label": "Condition report",
    "profile": "mix",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": true
      },
      "vocabulary": []
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": true,
    "darkOk": false,
    "deliverMost": true
  },
  "auto-dealer": {
    "preset": "auto-dealer",
    "label": "Dealership stock",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": true
      },
      "vocabulary": []
    },
    "panForgiven": false,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "wedding": {
    "preset": "wedding",
    "label": "Weddings",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "first_kiss",
          "label": "First kiss",
          "weight": 1.0
        },
        {
          "key": "ring_exchange",
          "label": "Ring exchange",
          "weight": 0.9
        },
        {
          "key": "vows",
          "label": "Vows",
          "weight": 0.8
        },
        {
          "key": "first_dance",
          "label": "First dance",
          "weight": 0.8
        },
        {
          "key": "confetti",
          "label": "Confetti",
          "weight": 0.7
        },
        {
          "key": "tears_or_laughter",
          "label": "Tears or laughter",
          "weight": 0.8
        },
        {
          "key": "bouquet_toss",
          "label": "Bouquet toss",
          "weight": 0.7
        },
        {
          "key": "walk_aisle",
          "label": "Walk down the aisle",
          "weight": 0.8
        }
      ]
    },
    "panForgiven": false,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "concert": {
    "preset": "concert",
    "label": "Concerts",
    "profile": "event",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "faces",
        "blocked": false
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "jump",
          "label": "Jump",
          "weight": 0.8
        },
        {
          "key": "crowd_surf",
          "label": "Crowd surfing",
          "weight": 0.8
        },
        {
          "key": "pyro",
          "label": "Pyro",
          "weight": 0.9
        },
        {
          "key": "guitar_raised",
          "label": "Guitar raised",
          "weight": 0.7
        },
        {
          "key": "singer_crowd",
          "label": "Singer and crowd",
          "weight": 0.7
        }
      ]
    },
    "panForgiven": true,
    "blur": "lenient",
    "levelCheck": false,
    "darkOk": true,
    "deliverMost": false
  },
  "wildlife-birds": {
    "preset": "wildlife-birds",
    "label": "Birds",
    "profile": "mix",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "in_flight",
          "label": "In flight",
          "weight": 0.8
        },
        {
          "key": "catch",
          "label": "Catch",
          "weight": 0.9
        },
        {
          "key": "animal_eye_contact",
          "label": "Eye contact",
          "weight": 0.7
        },
        {
          "key": "wings_apex",
          "label": "Wings at apex",
          "weight": 0.8
        },
        {
          "key": "interaction",
          "label": "Interaction",
          "weight": 0.6
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  },
  "landscape": {
    "preset": "landscape",
    "label": "Landscape",
    "profile": "mix",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "golden_hour",
          "label": "Golden hour",
          "weight": 0.6
        },
        {
          "key": "dramatic_light",
          "label": "Dramatic light",
          "weight": 0.7
        }
      ]
    },
    "panForgiven": false,
    "blur": "strict",
    "levelCheck": true,
    "darkOk": false,
    "deliverMost": true
  },
  "astro": {
    "preset": "astro",
    "label": "Astro",
    "profile": "mix",
    "detects": [
      {
        "key": "vehicles",
        "label": "Cars & bikes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "plates",
        "label": "Number plates",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "make_model",
        "label": "Make & model",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "sponsors",
        "label": "Sponsors & livery",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": true
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "milky_way_core",
          "label": "Milky Way core",
          "weight": 0.9
        },
        {
          "key": "meteor",
          "label": "Meteor",
          "weight": 0.9
        },
        {
          "key": "aurora",
          "label": "Aurora",
          "weight": 1.0
        }
      ]
    },
    "panForgiven": false,
    "blur": "lenient",
    "levelCheck": false,
    "darkOk": true,
    "deliverMost": false
  },
  "aviation": {
    "preset": "aviation",
    "label": "Aviation",
    "profile": "motorsport",
    "detects": [
      {
        "key": "vehicles",
        "label": "Aircraft",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "people",
        "label": "People",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "faces",
        "label": "Faces & eyes",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      {
        "key": "animals",
        "label": "Animals",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": null,
        "blocked": false
      }
    ],
    "reads": [
      {
        "key": "numbers",
        "label": "Race numbers",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "plates",
        "label": "Registrations",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "make_model",
        "label": "Aircraft type",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "sponsors",
        "label": "Operator & livery",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "vehicles",
        "blocked": false
      },
      {
        "key": "names",
        "label": "Names (from faces)",
        "on": false,
        "default": false,
        "overridden": false,
        "requires": "faces",
        "blocked": true
      }
    ],
    "moments": {
      "detect": {
        "key": "moments_detect",
        "label": "Spot moments",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": null,
        "blocked": false
      },
      "rank": {
        "key": "moments_rank",
        "label": "Let moments lift ranking",
        "on": true,
        "default": true,
        "overridden": false,
        "requires": "moments_detect",
        "blocked": false
      },
      "vocabulary": [
        {
          "key": "vapour",
          "label": "Vapour",
          "weight": 0.6
        },
        {
          "key": "afterburner",
          "label": "Afterburner",
          "weight": 0.9
        },
        {
          "key": "formation",
          "label": "Formation",
          "weight": 0.8
        },
        {
          "key": "gear_down",
          "label": "Gear down",
          "weight": 0.5
        },
        {
          "key": "landing_flare",
          "label": "Landing flare",
          "weight": 0.6
        },
        {
          "key": "knife_edge",
          "label": "Banked hard",
          "weight": 0.7
        }
      ]
    },
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  }
};
