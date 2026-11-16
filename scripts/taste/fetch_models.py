import os, sys, time
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
from huggingface_hub import hf_hub_download

MODELS = os.path.join(r'E:\code\photo\conrod\target\taste\models')
os.makedirs(MODELS, exist_ok=True)
want = [
    ('Qdrant/clip-ViT-B-32-vision', 'model.onnx'),
    ('Qdrant/clip-ViT-B-32-vision', 'preprocessor_config.json'),
    ('Qdrant/clip-ViT-B-32-vision', 'config.json'),
    ('onnx-community/dinov2-small', 'onnx/model.onnx'),
    ('onnx-community/dinov2-small', 'preprocessor_config.json'),
    ('onnx-community/dinov2-small', 'config.json'),
    ('immich-app/ViT-L-14__laion400m_e32', 'visual/model.onnx'),
    ('immich-app/ViT-L-14__laion400m_e32', 'visual/preprocess_cfg.json'),
    ('immich-app/ViT-L-14__laion400m_e32', 'config.json'),
]
for repo, fn in want:
    t = time.time()
    try:
        p = hf_hub_download(repo, fn, cache_dir=os.path.join(MODELS, '_hf'),
                            local_dir=None)
        print('%-40s %-30s %6.1f MB %5.1fs' % (repo.split('/')[-1], fn,
              os.path.getsize(p) / 1e6, time.time() - t), flush=True)
    except Exception as e:
        print('ERR', repo, fn, str(e)[:160], flush=True)
