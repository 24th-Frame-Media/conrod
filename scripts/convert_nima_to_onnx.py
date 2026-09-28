"""One-off: convert idealo/image-quality-assessment's NIMA MobileNet
"technical" weights (Apache-2.0) to ONNX, for crates/conrod-vision/src/quality.rs.

Run once, by hand, on a machine with TensorFlow available -- this is not part
of the app or CI, and TensorFlow is deliberately not a project dependency.

    pip install tensorflow tf2onnx keras-nima  # or clone idealo/image-quality-assessment
    python scripts/convert_nima_to_onnx.py --weights path/to/mobilenet_technical_weights.h5

Output: nima-mobilenet-technical.onnx, 224x224x3 NHWC float32 input
(Keras MobileNet preprocessing: pixel/127.5 - 1), 10-bin softmax output.
After converting: compute its SHA-256, upload it as a release asset, and put
the real sha256 and size into scripts/assets.json (see the TODO there).
"""

import argparse


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--weights",
        required=True,
        help="idealo NIMA MobileNet technical weights (.h5)",
    )
    parser.add_argument(
        "--out",
        default="nima-mobilenet-technical.onnx",
        help="output ONNX file",
    )
    args = parser.parse_args()

    # Imported lazily: these are one-off conversion dependencies, not
    # something the app or its CI ever needs installed.
    from tensorflow import keras
    import tf2onnx

    base = keras.applications.MobileNet(
        input_shape=(224, 224, 3), include_top=False, pooling="avg", weights=None
    )
    x = keras.layers.Dropout(0.75)(base.output)
    x = keras.layers.Dense(10, activation="softmax")(x)
    model = keras.Model(base.input, x)
    model.load_weights(args.weights)

    spec = (keras.Input((224, 224, 3), dtype="float32", name="image"),)
    tf2onnx.convert.from_keras(model, input_signature=spec, output_path=args.out)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
