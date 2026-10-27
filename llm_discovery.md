# LLM Discovery Notes

Observations about how Claude (and LLM-powered GitHub Actions) behave in practice.

---

## GitHub Action starts fresh from `main` — prior branch work is invisible

**Observed in:** Issue #15 (destructible Building cell)

The Claude PR Action runs fresh on each trigger. It reads the issue thread for context and the codebase at `main` for code state. If previous work was done on an unmerged branch, the action has no awareness of it.

**What happened:**
1. Branch `2215` had a correct house-mesh implementation (gable roof, custom vertices).
2. That branch was never merged into `main`.
3. A follow-up comment asked for a toolbar button.
4. The action spun a new branch from `main`, re-implemented the feature from scratch, and dropped the house mesh — it was never in `main` so there was nothing to preserve.

**The subtle part:** The issue thread *did* contain a comment describing the house mesh ("HouseMeshBuilder struct + build_house_mesh()..."), but the action treated it as historical summary, not as a requirement. It answered the latest request ("add a toolbar button") against the current codebase, not against the prior branch.

**Rule of thumb:** Merge the good branch before asking the action for the next step. Or explicitly tell it: "@claude build on branch `<name>` and add X."
