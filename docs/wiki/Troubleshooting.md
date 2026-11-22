# Troubleshooting

**November 2026**

[Home](Home) · Previous: [Recommended setups](Recommended-setups) · Next: [Roadmap](Roadmap)

## Start-up and scanning

| What you see | What it means | What to do |
|---|---|---|
| The splash stops on **Models** | A model is downloading, or the connection dropped | Wait; it resumes. Check disk space. |
| **Unfinished scans ready to resume** | A close or crash left albums half-scanned | Open the album and press **Resume**. |
| An album says **queued** | Another album is scanning; this one is waiting its turn | Wait, or press **Unqueue** to take it out of the line. |
| A scan is slow on the first pass | The detector, faces, and quality models load on first use | Later scans of the same album are faster. |
| **No graphics card was found** | Detection runs on the processor | Fine, just slower. Cloud vision still works. |

## Vision and identification

| What you see | What it means | What to do |
|---|---|---|
| Vision is off, reason **album** | The album switched it off | Turn it on for the album, or clear the override. |
| Vision is off, reason **settings** | The global switch is off | Settings > Identification > **Use a vision model**. |
| Vision is off, reason **no_service** | On, but no server is enabled | Add or enable a server. |
| Vision is off, reason **lanes** | This shoot type has nothing to ask | Normal; plates and numbers may still run. |
| **Vision model: Not set up** | No server added, or the switch is off | Add a server and switch it on. |
| **Ollama offline** | Ollama is not running | Start it from the tray, then refresh. |
| **401 / No auth / key refused** | Wrong key, trailing space, deleted, or credit limit | Copy the key again; check the provider site. |
| **402 / insufficient credits** | No money left | Add credit, or use Extra Usage / a free model. |
| **429 / rate limit** | Too many requests, or a free-model daily limit | Wait; on OpenRouter buy $10 of credit to raise the free limit. |
| **MissingSessionID** (OpenCode Go) | Old Conrod, or the wrong provider type | Update Conrod; choose **OpenCode Go (subscription)**, not plain OpenCode. |
| **does not support images** | The model cannot look at pictures | Pick a vision model from the guide. |
| Numbers say **read by OCR only** | Vision is off or failed for that car | Check the vision switch and server. |
| A race number looks like a sponsor | OCR over-read livery text | Add an entry list; Conrod then rejects unlisted OCR numbers. |

## Culling

| What you see | What it means | What to do |
|---|---|---|
| A sharp frame is not the keeper | Another frame in the burst is sharper, or you picked one | Choose your own pick; a hand pick wins. |
| A background car is dimmed | It is not the car under your autofocus area | Normal. It drives no rating or cull. |
| No keeper because all frames are soft | The album's blur bar is above every frame | Lower strictness, or open the best frame and rate it by hand. |
| Moments never appear | No working model, or below the 0.85 threshold | Check the Moments row; many vision-model moments show as *maybe*. |
| **Not enough video memory** (decision model) | Another program or Ollama is using the card, or the card is too small | Close it, choose 4-bit, or leave the decision model off; nothing is demoted. |

## Lightroom

| What you see | What it means | What to do |
|---|---|---|
| No **Plug-in Extras** menu | The plug-in is not installed, or Lightroom predates the install | Settings > Lightroom > **Install plug-in**, then restart Lightroom. |
| A package will not apply | Paths do not match, or the photos are in another catalog | Keep paths stable; check `%USERPROFILE%\.conrod\bridge\failed`. |
| Flags did not come back | The package is still in `in\`, or belongs to another album | Import it against the right album. |
| A photo was moved | Matching is by path | Re-export after the move. |

## Settings and files

| What you see | What it means | What to do |
|---|---|---|
| My old album changed after the wizard | It should not: albums keep their own copy of defaults | Check the album's **Shoot settings**; clearing an override follows the type. |
| I changed a global default and an album did not move | Expected | Global changes apply to new albums; changed an existing album explicitly. |
| Disk filling up | Thumbnails, previews, and crops | Settings > System > cache: clear previews or orphaned files. |
| I shared `settings.json` | Keys are stored unencrypted | Delete the provider keys on the provider's site. |

If a problem is not listed, check the scan log and identify feed for the failing step: LOAD, CROP, READ, VISION, ENTRY LIST, or RESULT.
