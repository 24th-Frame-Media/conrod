Assets shipped beside Conrod.exe (see rust/scripts/README.md).

  models/    yolo11s-960.onnx, face_detection_yunet_2023mar.onnx,
             dinov2-small-quantized.onnx, the plate detector + reader,
             ch_PP-OCRv4_det_infer.onnx + ch_PP-OCRv4_rec_infer.onnx
  exiftool/  exiftool.exe + exiftool_files/

Both are filled in by rust/scripts/fetch-release-assets.ps1 (CI) and are
git-ignored. A development checkout does not need them: the app looks in
~/.conrod/models first, then here.
