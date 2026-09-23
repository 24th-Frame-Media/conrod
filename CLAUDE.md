# Agent Policies & Model Orchestration Guidelines

This file is automatically loaded into the agent's context on every session. It defines mandatory operating policies, model selection tiering, and subagent concurrency rules.

---

## 1. Model Selection & Hierarchy

### Tier 1: Default & Routine Tasks (Speed & Efficiency)
* **Target Models**: Lower Gemini models (e.g., Gemini Flash, Gemini Flash-Lite) and Claude Sonnet.
* **Usage Scope**:
  * Routine coding, edits, syntax fixes, and refactoring within established patterns.
  * Codebase exploration, file search, and documentation lookups.
  * Unit test generation, minor debugging, and verification scripts.
  * Everyday pair-programming and tool/command execution.
* **Rule**: Always start with Tier 1. Do not deploy heavy frontier reasoning models to standard, straightforward tasks.

### Tier 2: Super Tough & Complex Tasks (Frontier Reasoning)
* **Target Models**: Claude Opus (or Gemini Pro / highest-tier reasoning models).
* **Activation Criteria**:
  * Deep, multi-layered architectural problems and complex system design.
  * Intractable bugs, tricky concurrency/race conditions, or subtle memory issues that Tier 1 cannot solve.
  * High-complexity algorithmic challenges or domain-specific mathematical problems.
* **Rule**: Reserve Opus strictly for super tough, complex problems where smaller models fail or are clearly ill-equipped to resolve. Never burn Tier 2 resources on routine mechanical edits or simple tasks.

---

## 2. Subagent Concurrency & Delegation Limits

### Strict Concurrency Cap (Max 3 Subagents)
* **Hard Upper Bound**: **Maximum 3 concurrent subagents** active at any given moment (`active_subagents <= 3`).
* **Minimalism**:
  * Default to 0 subagents (solve tasks directly within the main session).
  * Use subagents in very small amounts only when task isolation or true parallel investigation is beneficial.
  * Never spawn subagent swarms or wide speculative fan-outs.
  * If 1 subagent is sufficient, do not launch 2. If 2 suffice, do not launch 3.

### Subagent Model Allocation
* **Routine Delegation**: Subagents performing research, codebase search, or background checks must use or inherit Tier 1 models (`flash_lite`, `flash`, or Sonnet).
* **Escalated Delegation**: Only assign Tier 2 (Opus / high-tier reasoning) to a subagent if it is tasked with an isolated, deep algorithmic/reasoning challenge that Tier 1 cannot handle.

### Subagent Lifecycle Discipline
1. **Crisp Scoping**: Give each subagent a tightly bounded, single-purpose objective.
2. **Concise Synthesis**: When a subagent finishes, synthesize its core findings cleanly into the main context without token bloat or raw log dumping.
3. **Prompt Cleanup**: Terminate or conclude subagents promptly once their task is complete.

---

## 3. Execution & Escalation Protocol

1. **Autonomous Investigation First**: Use available search, file inspection, and diagnostic tools to understand root causes before making assumptions.
2. **Transparent Escalation**: If escalating an intractable problem to a Tier 2 model (Opus):
   - Summarize the exact core problem and constraints.
   - Outline what was attempted and where Tier 1 hit an impasse.
   - State the specific deep-reasoning hypothesis or architectural dilemma requiring Tier 2 attention.
3. **Rigorous Verification**: Always validate code changes using workspace build and test tooling before declaring a task complete.
