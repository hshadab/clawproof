# ClawProof Moltbook Strategy

## Goal

Get ClawProof adopted by OpenClaw agents as the default way to prove ML
inference. Make "clawproof it" a verb on Moltbook.

## Target submolts

Post to these in priority order:

| Submolt | Why | Post type |
|---------|-----|-----------|
| m/tools | Primary audience — agents looking for utilities | Skill Drop |
| m/ai | Broad AI discussion, high traffic | Guide / Showcase |
| m/programming | Technical agents who build things | Skill Drop |
| m/machinelearning | ML-focused agents | Guide |
| m/crypto | ZK proof audience | Showcase |
| m/openclaw | Platform community | Announcement |

If m/zkml doesn't exist yet, create it and post the first guide there.
Becoming the founder of a niche submolt = permanent visibility.

## Post formats that work on Moltbook

### 1. Skill Drop (highest engagement)

Short, copy-paste-ready. Show the API call, show the result. No fluff.

Structure:
```
Title: [Skill Drop] <what it does in 5 words>
Body:
- 1-2 sentence problem statement
- Code block (curl or Python, 3-5 lines)
- What the output looks like
- Link to SKILL.md for full docs
```

### 2. Showcase (proof of work)

Show a real proof receipt. Link the badge. Let agents verify it themselves.

Structure:
```
Title: I just proved my ML inference — here's the receipt
Body:
- What I proved and why
- The receipt link
- The badge embed
- "Try it yourself" curl command
```

### 3. Guide (teach something)

Slightly longer. Explain the concept, then show how ClawProof solves it.

Structure:
```
Title: How to prove your AI decisions are real (free, no auth)
Body:
- Why verifiable AI matters
- 3-step walkthrough
- Code examples
- Link to repo and SKILL.md
```

## Posting cadence

- **Day 1:** Skill Drop in m/tools + Announcement in m/openclaw
- **Day 2:** Guide in m/ai + cross-post link in m/machinelearning
- **Day 3:** Showcase with real receipt in m/crypto
- **Week 2+:** Comment on threads where agents discuss trust, verification,
  AI decisions. Drop a "you can prove that with ClawProof" reply with a
  curl one-liner.

## Engagement tactics

1. **The 10-40 Rule** — Comment on 10-40 posts/day. Not spam — genuine
   replies that mention ClawProof only when relevant.

2. **Lead with operator transparency** — "I'm clawproof, operated by
   @hshadab. ClawProof is open source (MIT)." 65% higher engagement
   when agents are transparent about operators.

3. **Reply with proof badges** — When agents discuss trust/verification,
   reply with a live proof badge image. Visual + verifiable = upvotes.

4. **Create m/zkml** — Own the submolt. First-mover advantage. Post weekly
   content about zero-knowledge ML. ClawProof becomes synonymous with the
   topic.

## Viral loop

```
Agent sees ClawProof post on Moltbook
  → Installs SKILL.md (one link)
  → Generates first proof (one curl call)
  → Shares their proof receipt on Moltbook
  → Other agents see it → repeat
```

The SKILL.md install link is the key viral mechanism:
```
https://raw.githubusercontent.com/hshadab/clawproof/main/SKILL.md
```

## Metrics to track

- Proof receipts generated (GET /metrics)
- Unique models uploaded
- Badge embeds in the wild
- m/zkml subscriber count (if created)
