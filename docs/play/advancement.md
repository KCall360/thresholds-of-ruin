# Player advancement and respec

Advancement is available only to the connection currently controlling its own
character. Attachment, a player credential, and wizard inspection alone do not
grant access to the private purchase ledger or previews. Browsing is free and
works while ordinary work is in progress.

## Open and navigate

- Text, including `--script`: enter `advancement` (or `advance`), then prefix
  each command below with `advancement`.
- ASCII: press **F10**. Type a command below and press Enter. Up/Down scroll
  the current page; Page Up/Page Down request the previous/next server page.
  Escape closes the screen without acting.
- Headless: send `{"type":"advancement","command":"overview"}`. Replace
  `overview` with any command below; wait for the usual `ready` response.
  Frames include `advancement` display lines and the current typed
  `advancement_page`. The [headless guide](../reference/headless.md) explains
  ordinary action acknowledgements and state updates.

| Command | Result |
| --- | --- |
| `overview` | Starting skills, points, earned/held XP, next threshold, explicit future growth source, provisional timing, readiness and respec progress |
| `ledger` | Existing purchases, their HD owner/pool/source and Active/Dormant state |
| `balances` | Granted, spent and unspent points in each exact HD/pool |
| `skills`, `talents`, `expertises` | Server catalog IDs and names; catalog membership does not establish eligibility |
| `intellect-expertises` | Retained Intellect thresholds and their choices |
| `intellect-expertise <threshold> <expertise ID>` | Preview a non-specialist expertise for an eligible retained threshold; spends no HD pool point |
| `next`, `previous` | Another bounded page of the current section |
| `source racial`, `source class <class ID>` | Preview a future HD source; confirm applies retained XP when eligible |
| `purchase <HD> <pool ID> <skill\|talent\|expertise> <ID> [form <form ID>]` | Preview that exact purchase; a choice-bearing talent requires its listed form ID |
| `remove <HD> <purchase number>` | Preview removing an existing purchase without a replacement |
| `replace <HD> <purchase number> <pool ID> <skill\|talent\|expertise> <ID> [form <form ID>]` | Preview replacing an existing purchase at its same HD owner, retaining the exact form choice |
| `details <section>` | Preview section: `summary`, `removed`, `refunds`, `balances`, `skills`, `talents`, `expertises`, or `conditions` |
| `supplies` | Eligible carried or explicitly touch-reachable known ration sources |
| `confirm [using carried\|<name or #item>]` | Commit the exact retained proposal through ordinary action admission |
| `resume` | Resume the exact paused respec shown by the current overview, when permitted |
| `cancel` | Cancel that respec, when permitted, releasing its unspent ration |

HD and purchase numbers in these controls start at **1**. The protocol uses
zero-based owner and source indices; the client preserves those exact operands.
Intellect threshold numbers are already one-based and are not zero-based HD
wire owners. Body-package pools are locked; the live presets' first purchasable
class owner is HD 2. See [progression ownership](../reference/progression.md).
Copy pool and selection IDs from the server's pages. No pool is chosen silently.
A talent catalog keeps each root on one row with up to sixteen named form options.
Use `talent <root ID> form <form ID>` only for a root that lists choices. The
selected form belongs to that purchase; dormancy and restoration preserve it.

## Review before confirming

A preview shows the exact proposal, dependent removal/refund counts, effective
Health and injury, death, Health recovery, named resource balances/maximums,
and canceled resource holds. The details sections expose **every** dependent
removal, gross refund, resulting net point balance, permanent skill, resulting
Active/Dormant talent, and removed condition/application/stack count. Use next
and previous until the relevant section ends. Refund-only removal is an explicit
null replacement, not a replacement selected by the client.

The server owns prices, dependency closure, refunds and all resulting values.
A catalog row is not a promise that a purchase is eligible. A preview does not
reserve points or supplies. Page payloads are limited to 32 rows and the complete
escaped response must fit both 32 KiB and the negotiated frame size. Small
byte-limited pages still have exact continuations; clients retain at most four
payload pages and requery older pages as needed.

A changed observation/revision or readiness context marks the proposal stale.
If the observation changes while an ASCII query is queued, the screen stays open
and reports that the request was not sent. Request `overview` again and explicitly
preview the intended change from that fresh owner-authorized page. The client
does not automatically retry a purchase or replacement.
Open overview and explicitly preview again; confirm never substitutes a newer
basis. Reset, branch change, control loss and reconnect discard private pages
and drafts. Another controlled actor cannot inherit them.

## Earned XP and future growth

XP belongs to the actor whose effect kills a living target. An ally delivering
the fatal effect receives the credit. The server calculates the reward and
applies each eligible earned HD immediately using the character's declared
future source. Its point grants remain unspent for ordinary purchases.

Use `source racial` or `source class warrior` (or another valid class ID) to
preview a change, then `confirm`. Choosing a source spends no ration and does
not replace the saved choices of a lost HD. Disabled, missing or ineligible
future sources retain earned XP until eligible growth can apply. The overview
shows total earned, spent and held XP, its next threshold and peak threat.
XP policy numbers are provisional.

## Rations, work and interruptions

Fresh purchases, source selection and respec require ordinary readiness and no existing active or
paused work. Finish or cancel that work and wait out any retained recovery debt.
For an existing paused respec, use the explicit `resume` command rather than
starting over. ASCII F8/F9 use the same exact respec controls while its screen is
open; ordinary intention controls remain available outside it.

Respec uses exactly one ration. Carried inventory is the default. To choose an
exact source, use its displayed name or opaque `#item` handle. Ground rations are
available only when the server discloses the authored carried-or-touch-ground
policy and current contact. ASCII `confirm` opens the existing finite-supplies
picker: Up/Down select, Space marks one exact source, Enter confirms, Escape
returns without acting. `confirm using ...` supplies an explicit choice directly.
The client never sends a raw item ID, a quantity override, a price or a refund.
Visible physical quantities do not reveal reservations or guarantee admission.

Overview shows perceived danger and the remaining received-Health-loss quiet
interval. These are disclosed facts, not a claim that no hidden danger exists.
Respec progress shows server-reported remaining/total active ticks; frozen time
and a pause do not accrue work. A benign pause may retain exact work for resume.
Danger, positive received Health loss or a changed build can discard preparation;
retrying then starts a fresh full duration. The ration is consumed only when the
server completes the replacement atomically. Cancellation releases unspent stock.
