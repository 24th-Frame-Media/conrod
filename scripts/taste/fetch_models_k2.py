import os, sys, time
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
from huggingface_hub import hf_hub_download

M = r'E:\code\photo\conrod\target\taste\models\_hf'
want = [
    ('86Cao/IQA-ONNX-Models', 'musiq_model.onnx'),
    ('86Cao/IQA-ONNX-Models', 'musiq_model.onnx.data'),
    ('86Cao/IQA-ONNX-Models', 'clipiqa_model.onnx'),
    ('86Cao/IQA-ONNX-Models', 'clipiqa_model.onnx.data'),
    ('86Cao/IQA-ONNX-Models', 'clip_model.onnx'),
    ('86Cao/IQA-ONNX-Models', 'clip_model.onnx.data'),
    ('86Cao/IQA-ONNX-Models', 'liqe_model.onnx'),
    ('86Cao/IQA-ONNX-Models', 'liqe_model.onnx.data'),
    ('86Cao/IQA-ONNX-Models', 'text_features.json'),
    ('nocturna/topiq-nr-koniq-onnx', 'model.onnx'),
    ('fsw/aesthetic-predictor-v2-5_onnx', 'aesthetic_predictor_v2_5.onnx'),
    ('fsw/aesthetic-predictor-v2-5_onnx', 'convert_to_onnx.py'),
]
for repo, fn in want:
    t = time.time()
    try:
        p = hf_hub_download(repo, fn, cache_dir=M)
        print('%-28s %-34s %7.1f MB %5.1fs' % (repo.split('/')[-1][:28], fn,
              os.path.getsize(p) / 1e6, time.time() - t), flush=True)
    except Exception as e:
        print('ERR', repo, fn, str(e)[:120], flush=True)
