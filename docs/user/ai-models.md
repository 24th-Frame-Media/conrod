<!-- Matches the Settings "AI models" page from branch wt/ai-models (e49623f). Re-check the menu names and the
     prices below after that branch is merged and before each release. Prices and limits checked 22 November 2026.
     Parts marked "(coming)" describe features designed in docs/plans/burst-pipeline.md (built-in vision model,
     vision model judging moments, the setup-wizard step, the OpenRouter key test in the vision server form). Remove the
     marks as each one ships. The OpenRouter decision model is no longer offered for photos (decided 2026-11-22). -->

# AI models in Conrod: what they do and how to set them up

Conrod uses several models to cull and label photos. Most run on your PC and need no setup. The larger **vision model**
can run in the cloud. The **on-device decision model** runs on your PC; OpenCode Go can also provide JEV for moment checks.
(coming) The vision model can also do the decision model's job, so you do not need both.

You can turn off every model and still get sharpness checks, bursts, and suggested keepers. Models add vehicle detection,
number and plate reads, car identification, and moment spotting.

Prices and limits were checked on **22 November 2026**. Check the provider's page before adding money.

---

## 1. What each model does

**Settings > AI models** lists each model, where it runs, its status, and its switch.

| In Settings it is called | What it does for you | Where it runs | Needs |
|---|---|---|---|
| **Car & person detector (YOLO)** | Finds the cars, bikes, trucks and people in every photo. Everything else starts from here: it decides what the "subject" is, and which car is the main one (the one your camera focused on). | Your PC, using the graphics card if it has one (any recent card), otherwise the processor (slower). | About 40 MB, installed automatically. Always on. |
| **Plate finder + plate reader** | Finds a number plate on the car and reads its letters and digits. | Your PC | About 12 MB, installed automatically. |
| **Number/text reader (OCR)** | Reads race numbers, signs and sponsor boards as text. It fills in a number only when the vision model did not see one. | Your PC | About 15 MB, installed automatically. |
| **Faces/eyes** | Finds faces and checks whether eyes are open, so a shut eye can lower a portrait's rating. It can also help suggest names. Nothing leaves your PC. | Your PC | About 43 MB, installed automatically. Turn on with **People**. |
| **Sharpness/quality** | Checks how sharp the subject is. The main sharpness check is a measurement of the picture itself and needs no download. The optional quality model scores a subject's technical quality and is used only to choose between two equally sharp frames. | Your PC | The optional model is a small extra download. Off by default. |
| **Vision model** | Looks at a cropped picture of a car and reads **make, model, colour, team and race number**. This is the model that "knows what a car is". It is the slowest part of a scan. | (coming) **Built into Conrod** ("This PC (built-in)"), no extra program needed. Or your PC through **Ollama** (a free program you install yourself). Or the cloud (OpenRouter, OpenCode Go, and others). | Built-in: about 5.2 GB of download and about 6 GB of free video memory (the graphics card's own memory). Ollama: about 6 GB of each. Cloud: a key. |
| **Decision model** | Answers yes/no questions about a photo: tyre smoke? a kiss? the peak of a jump? It helps choose between equally sharp frames. Optional. | On-device: your PC's graphics card. Cloud moments: JEV through OpenCode Go. | About 5.5 GB download and about 6.5 GB of free video memory for the on-device model (or about 4.5 GB in the smaller "4-bit" version). |

**The vision model can stand in for the decision model.** (coming) If the on-device model does not fit or you do not want
its download, the vision model can answer the same yes/no questions using the full photo and car crop in the car-reading
request. Its answers are less precise, so more show as "maybe". The setup wizard chooses based on your graphics card and
keys; you can change the choice later.

The "same car?" matcher also runs on your PC (about 23 MB) and groups the same car across bursts. An optional "taste"
ranker is available.

### What "vision off" means

If the vision model is **off** (or no server is set up or reachable), Conrod still finds cars, checks sharpness,
groups bursts and suggests keepers. It still reads **plates and race numbers** with the plate and text readers. It
does **not** read make, model, colour or team, and numbers are less certain: they are marked "Number read by OCR
only". Nothing is wrong; it just does less. The scan screen tells you when vision is off and why.

### How good are they?

In Conrod's motorsport tests, the local vision model read the race number correctly about
**83%** of the time and the exact model about **91%** of the time. At a car meet it was weaker: the make was right about
**73%** of the time and the model only about half the time. Always glance at what it reads before you rely on it.
The cloud models listed below are mostly **untested** by us; only one has been tried (see section 6).

---

## 2. What stays on your PC and what can go to the cloud

| | Stays on your PC | Can go to the cloud (only if you choose) |
|---|---|---|
| Your photo files (RAW, JPEG) | Always. Conrod never uploads whole files. | Never |
| Detector, plates, text reader, faces/eyes, sharpness, grouping | Always | Never |
| **Vision model** | If you pick "this PC" (built-in or Ollama) | A **cropped picture of one car** (up to about 1,500 pixels on its long side), plus a short instruction. At shoots where Conrod reads jersey numbers, the crop is of a **person**. (coming) If the cloud vision model also answers the yes/no moment questions, it gets a **small copy of the whole photo** (about 640 pixels wide) as well. |
| **On-device decision model** | Always. | Never |
| **JEV moment checks** | Never | Through OpenCode Go, if enabled. |
| Your key | Stored on your PC in Conrod's settings file, **not encrypted** | Sent to the company that owns it, to prove it is you |

If you shoot weddings, children, or anything with a privacy agreement, keep the vision model on your PC or turn it
off.

---

## 3. Which setup is right for you?

### (a) A good graphics card: everything on your PC

For a card with 12 GB of video memory or more. No usage fees; photos stay on the PC.

1. (coming) In Conrod, Settings, **AI models**, press **Download the built-in vision model** (about 5.2 GB). Nothing else
   to install. (Today: install **Ollama**, ollama.com, free, let it download `qwen2.5vl:7b`, and add it as your vision
   server, section 7.)
2. With a 16 GB card or more, also turn on **Use the decision model** and let it download (about 5.5 GB). With 12 to 16 GB
   you can skip it: the vision model answers the moment questions too (see section 1).

A local vision read takes about 10 seconds per car. The built-in model aims for 5 to 8 seconds, but has not been measured.
Large shoots take time; leave Conrod running.

### (b) A laptop or a weak graphics card: use the cloud

For under about 6 GB of video memory or no graphics card. Cloud reads are quick and cost cents.

Try OpenCode Go first: it is probably the best bang for your buck. Its subscription allowance covers vision reads and JEV moment checks (`jev-1.13` or `jev-1.13-free`) with one key, instead of per-token billing.

1. Set up **OpenRouter** (section 4) or **OpenCode Go** (section 5) as your vision server.
2. Leave the on-device **decision model** off if you do not want its download. You can select JEV through OpenCode Go for
   moment checks; the cloud vision model can also judge moments (coming).

Detection will run on the processor if there is no graphics card, which is slower: Conrod tells you at start-up.
With no graphics card and no key, Conrod still reads plates and race numbers, but not make or model.

### (c) Mixed: a mid-range card (8 GB) with a cloud helper

For an 8 GB card, common in gaming laptops. The vision and decision models do not both fit on the card. If you have a key:

1. Vision model in the **cloud** (OpenRouter or OpenCode Go), so the card is free for the detector.
2. Decision model on your **graphics card**, in the smaller 4-bit version if Conrod offers it. Or skip it and let the
   cloud vision model answer the moment questions (a bit less precise, much faster).

With **no key**: use the built-in vision model (coming) and let it answer the moment questions too. It is the only big
model on the card, so everything fits. It is slow, a few hours for a very big shoot, which is the price of keeping every
photo on your PC.

Conrod chooses between these setups when a scan starts and tells you which it picked.

---

## 4. OpenRouter, step by step

OpenRouter is one account that can use many companies' models. You pay only for what you use.

### 4.1 Sign up

1. Go to **openrouter.ai** and press **Sign up** (Google, GitHub or email).

### 4.2 Add credits

1. Open the **Credits** page (openrouter.ai/settings/credits) and add money. Five or ten dollars lasts a long time
   (see the table in 4.4).
2. OpenRouter keeps a fee of **5.5% (at least $0.80)** when you buy credits.
3. Buying **$10 or more** of credits in total also raises the daily limit on **free** models (20 requests a minute, 50 a day below $10, 1,000 a day from $10). Paid models, like the ones below, have no such daily limit.

### 4.3 Make a key

A key is a long password that lets Conrod use your account.

1. Open **Keys** (openrouter.ai/settings/keys) and press **Create key**.
2. Name it "Conrod". **Set a credit limit** (for example $5) so a mistake can never cost more than that.
3. Copy the key now (it starts with `sk-or-`). OpenRouter shows it only once. Keep it private.

### 4.4 Pick a vision model

Open openrouter.ai/models and filter **Input: Image** so you only see models that can look at photos. Prices are per
million "tokens" (roughly, pieces of text and picture). The last three columns are the cost for **1,000 photos**.

How the cost is worked out. A **read** is one cropped car picture sent, and a short answer back (about 1,900 tokens in and
150 out). Today Conrod reads the **keeper** of each burst, about **160 reads per 1,000 photos**. (coming) The new burst
scan reads each car in a burst until two reads agree, at most three: about **300 reads per 1,000 photos**. (coming) If the
vision model also answers the yes/no moment questions for every sharp photo, that adds about **300 quick checks** (a small
copy of the whole photo plus the crop, about 1,100 tokens in and 60 out). Reading **everything** would be about 1,100 reads.

| Model (type this into Conrod) | Price per million, in / out | Today: keepers only | (coming) Per burst | (coming) Per burst + moment checks | Note |
|---|---|---|---|---|---|
| `z-ai/glm-5.3-flash` | $0.04 / $0.078 | about **1.4 cents** | about **2.6 cents** | about **4 cents** | Cheapest. Same family as the one model Conrod has tried in the cloud (section 6). A good first choice. |
| `google/gemini-2.5-flash-lite` | $0.10 / $0.40 | about 4 cents | about 7.5 cents | about 12 cents | Fast, well known. Untested by us. |
| `anthropic/claude-haiku-5.5` | $0.10 / $0.50 | about 4 cents | about 8 cents | about 12 cents | Untested by us. |
| `qwen/qwen3-vl-8b-instruct` | $0.25 / $0.75 | about 9 cents | about 18 cents | about 27 cents | Same family as the local model. Untested by us. |
| `google/gemini-3.1-flash-lite` | $0.25 / $1.50 | about 11 cents | about 21 cents | about 32 cents | Untested by us. |
| `qwen/qwen2.5-vl-72b-instruct` | $0.80 / $1.00 | about 27 cents | about 50 cents | about 78 cents | The bigger sibling of the local model, and Conrod's suggested starting model in the list. About 20 times the cost of the cheapest. Untested by us. |

Real bills can be **two or three times higher**: some models turn one photo into many tokens, and some "think" out loud
before answering and charge for it. Watch the **Activity** page on openrouter.ai after your first scan. Prices are from
OpenRouter's public list on 22 November 2026, plus the 5.5% fee on the money you add.

### 4.5 The on-device decision model is not available through OpenRouter

The on-device **decision model** runs on your graphics card. OpenRouter's Perplexity decision model accepts text and JSON,
not images, so Conrod does not send photos to it. OpenCode Go supports JEV for moment checks with the same key used for
vision reads.
### 4.6 Privacy

- OpenRouter says it does **not log your prompts or pictures** unless you turn on logging yourself (it offers a 1%
  discount for that: leave it off).
- It passes your request to a model company. OpenRouter says it avoids companies that train on your data unless you allow
  it in **openrouter.ai/settings/privacy**. Look at that page and the "zero data retention" option.
- Remember what is sent: car crops and, (coming) when the cloud vision model also answers moment questions, a small copy of the whole photo. See section 2.

### 4.7 Put it into Conrod

1. Open Conrod, **Settings**, then the **Identification** tab, then **AI models**.
2. In the **Vision model** row press **+ Add server**. For **Type** choose **OpenRouter**. Paste your key. For **Model** pick
   one from the list (press the refresh arrow if it is empty) or choose **Other** and type a model name from the table above.
3. (coming) Press **Test key**. It says "OpenRouter key works" and shows your key's name. This check **does not spend any
   credit**. (In the version that is out now, the **Test OpenRouter key** button sits in the **Decision model** row next to
   the **OpenRouter key** field: paste the key there, test it, then press **Use the decision model's OpenRouter key** in the
   vision form.)
4. Press **Add**, make sure **Use a vision model** is switched on, then press **Save changes**.
---

## 5. OpenCode Go (a monthly subscription)

### 5.1 What it is

OpenCode Go is a **fixed monthly price** for a set of models, sold mainly to people who use AI coding assistants.
The monthly allowance replaces per-photo billing. Conrod can use Go for vision reads and JEV moment checks.

| Plan | Price | |
|---|---|---|
| **Go** | **$10 a month** | Enough for Conrod many times over |
| Go Plus | $40 a month | Higher limits |
| Go Teams | $20 per person per month, 2 people minimum | Each person has their own limits |

### 5.2 How to get a key

1. Go to **opencode.ai/go** and sign in to the **OpenCode Console**.
2. Subscribe to **Go** and add your billing details.
3. Copy your **API key** from the Console.

### 5.3 Which models can read photos

Not every model in Go can look at pictures. Pictures work with, among others: **GLM-5.3-Flash**, **Qwen3.8 Flash**,
**Qwen3.7 Plus**, **MiMo-V2.6-Flash**, **MiniMax M3.1 Flash**, **Claude Haiku 5.5**, **GPT-6 Luna** and **Gemini 4 Argon**.

Use **GLM-5.3-Flash** (`glm-5.3-flash`). It is the only one Conrod's team has tried; it read a test car's number correctly.
For moment checks, OpenCode Go offers JEV as `jev-1.13` and `jev-1.13-free`.
**Do not use DeepSeek V4.1 Flash for cars:** in our test it kept thinking and never gave an answer.

### 5.4 Limits and cost

- Your plan has a monthly allowance measured in dollars at the models' list prices. On Go, it is **$60 of GLM-5.3-Flash use**
  a month. The allowance also has a **5-hour** window (a fifth of the month) and a **weekly** window (half the month, resets
  Monday 00:00 UTC).
- At our assumed 1,900 tokens in and 150 out per read, GLM-5.3-Flash is about **0.04 cents per read** on Go, so $60 covers well
  over **100,000 reads** a month, far more than one photographer needs. OpenCode's own estimate for the model is about
  6,300 requests per 5 hours (that figure is for coding-style requests).
- If you reach a limit you can keep using free models, or turn on **Extra Usage** (it spends credit you added).

### 5.5 Be aware

- Go is sold for **coding assistants**. OpenCode says clients must identify themselves and send a session ID, and that
  traffic is monitored. Conrod does both, and it worked in our test on 18 November 2026. OpenCode does **not** promise a photo
  app is supported, and its rules may change. If Go stops working, use OpenRouter instead.
- **Privacy:** by default, Go blocks models that train on your requests, and allows regions in **Europe and the United
  States**. GLM-5.3-Flash is listed with zero data retention. Check **Settings, Privacy** in the OpenCode Console.

### 5.6 Put it into Conrod

1. **Settings**, **Identification** tab, **AI models**, **Vision model** row, **+ Add server**.
2. For **Type** choose **OpenCode Go (subscription)**. Paste the key.
3. For **Model**, press the refresh arrow: if a list of models appears, your key works. Pick **GLM-5.3-Flash**.
4. Press **Add**, make sure **Use a vision model** is on, and press **Save changes**.

There is no separate "Test" button for OpenCode Go. A model list that loads is the check. (Do not choose plain
**OpenCode (Console credits)**: that one bills per use and stops when your credit runs out.)

---

## 6. What has been tried

- **Local, on a laptop with an 8 GB graphics card:** `qwen2.5vl:7b` through Ollama, about 10 seconds per car, the best of the
  local models we tested.
- **Built-in vision model:** (coming) the same model, Qwen2.5-VL 7B, built into Conrod. It has not been measured yet: we
  expect 5 to 8 seconds per car and about 6 GB of video memory, and we will only make it the default once its readings match
  Ollama's.
- **Cloud:** only `glm-5.3-flash` through OpenCode Go has been tried, on one car. Everything else in the tables is a
  sensible guess from price and model family. If you try one, spot-check the first 20 readings.

---

## 7. Where everything is in Conrod

**Settings**, then the **Identification** tab, then **AI models**.

- Each model has a row with a plain description, **where it runs** ("This PC", "This PC's graphics card", "Cloud"),
  its state (**Ready**, **Downloading**, **Off**, **Not set up**), and a switch.
- **Vision model** row: (coming) **Download the built-in vision model**, and **+ Add server** for a local Ollama or a
  cloud provider. You can add more than one; Conrod shares the work between them. **Use a vision model** is the on/off switch.
  (coming) **Moments are judged by**: Automatic (the decision model if it fits, otherwise the vision model), Decision model,
  or Vision model.
- **Decision model** row: **Use the decision model** switch and **Precision** (8-bit or the smaller 4-bit). The on-device
  model runs on your graphics card.
- (coming) The first-run **setup wizard** has an "AI helpers" step that picks sensible choices from your graphics card and
  any key you paste, and shows how long a 1,000-photo shoot should take. **Re-check my setup** in Settings runs it again.
- **Save changes** at the bottom saves everything.
- An album can follow this switch or set its own. When vision is off for an album, the scan screen says so ("vision model off for this album").

---

## 8. If something goes wrong

| What you see | What it usually means | What to do |
|---|---|---|
| "Vision model: Not set up" | No server added, or the switch is off | Add a server (section 4.7 or 5.6) and switch **Use a vision model** on |
| "Ollama offline at http://localhost:11434" | The Ollama program is not running | Start Ollama (it sits in the system tray), then press the refresh arrow |
| "No auth", **401**, "key refused" | The key is wrong, has a space on the end, was deleted, or hit its credit limit | Copy it again; check the key on the provider's website |
| **402**, "insufficient credits" | No money left | Add credits (OpenRouter), or turn on Extra Usage or top up (OpenCode) |
| **429**, "rate limit" | Too many requests too fast, or a free-model daily limit | Wait a few minutes. On OpenRouter, buy $10 of credits to raise the free limit. On Go, you reached a 5-hour or weekly window |
| "does not support images" or empty answers | The model cannot look at pictures | Pick one from the tables above, or filter by **Input: Image** on openrouter.ai/models |
| **400**, "MissingSessionID" (Go) | A very old Conrod, or the wrong OpenCode type | Update Conrod; choose **OpenCode Go (subscription)**, not plain OpenCode |
| Numbers say "Number read by OCR only" | Vision is off or failed for that car | Check the vision switch and server; the scan screen says why |
| "Not enough video memory: X free, needs Y" (decision model) | Another program (a game, a video, Ollama) is using the card, or the card is too small for both models | Close it, choose the 4-bit version, or leave the on-device model off. You can select JEV through OpenCode Go for moments. (coming) The vision model can answer moment questions too. The scan works without either |
| (coming) "Built-in vision model: not enough video memory" | The card has under about 6 GB free | Close other programs that use the graphics card, use a cloud vision server, or leave vision off |
| (coming) The built-in vision model download stops or fails | A dropped connection or full disk (it needs about 5.2 GB free) | Press download again: it carries on where it stopped and checks the file when done |
| "No graphics card was found" | Detection runs on the processor | Fine, just slower. Cloud vision still works |
| Moments such as "tyre smoke" never appear | No decision model on your PC and no working vision model, or the moment threshold is not reached | Check the Moments row in AI models; the scan screen says who is judging moments. Many moments show as "maybe" when the vision model judges them |
| The bill is bigger than expected | A "thinking" model, a big model, or "read everything" | Check **Activity** on openrouter.ai; set a **credit limit** on the key; switch to `z-ai/glm-5.3-flash`; read keepers only |
| A scan is slow | Local vision is about 10 seconds per car | Use a cloud server for the vision model, or leave it running |

Your key is saved in Conrod's settings file on your PC without encryption. Use a key with a **credit limit**, and delete
the key on the provider's website if you ever share the file or the PC.

---

## 9. Sources (checked 22 November 2026)

- OpenRouter model list and prices: openrouter.ai/api/v1/models (queried that day)
- OpenRouter credits, fees, privacy: openrouter.ai/docs/faq
- OpenRouter free-model and rate limits: openrouter.ai/docs/api/reference/limits
- Why the decision model is not offered through OpenRouter (images not yet supported): openrouter.ai/perplexity/pplx-decider-flash-v1.3
- Built-in vision model files and sizes: huggingface.co/ggml-org/Qwen2.5-VL-7B-Instruct-GGUF (Apache-2.0)
- OpenCode Go plans, limits, privacy: opencode.ai/go and opencode.ai/docs/go
- OpenCode Go model list (which accept pictures, prices): opencode.ai/inference/go/v1/models
- Conrod's own test numbers: the project's model notes (qwen2.5vl:7b rounds 1 and 2; GLM-5.3-Flash through Go on 18 November 2026)
