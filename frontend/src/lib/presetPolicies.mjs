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
        "blocked": false
      }
    ],
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
    "panForgiven": true,
    "blur": "moderate",
    "levelCheck": false,
    "darkOk": false,
    "deliverMost": false
  }
};
