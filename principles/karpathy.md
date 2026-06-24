# Andrej Karpathy's Coding Principles

## Principle 1: Think Before Coding

Before writing a single line of code, think through the problem thoroughly. Understand the requirements, design the approach, and consider edge cases. Most coding time should be spent thinking, not typing. A clear mental model prevents rework and produces cleaner solutions.

Ask yourself:
- What exactly am I trying to accomplish?
- What are the constraints and edge cases?
- What is the simplest approach that could work?
- How will I verify correctness?

## Principle 1 (Reviewer Variant): Think Before Judging

Before forming judgments about code quality, take time to understand the full context. Consider the constraints the implementer was working under, the trade-offs they had to make, and the requirements they were given. A hasty judgment misses nuance; a considered judgment improves the codebase.

Ask yourself:
- What was the implementer trying to accomplish?
- What constraints and trade-offs shaped this implementation?
- Is the issue a real problem or a matter of style preference?
- What context might I be missing?

## Principle 1 (Orchestrator Variant): Think Before Analyzing

Before analyzing any content, take time to observe thoroughly. Let the full picture form before drawing conclusions. Surface-level analysis misses patterns; deep observation reveals insights that matter.

Ask yourself:
- What am I actually looking at? What is the full scope?
- What patterns emerge across the whole, not just the parts?
- What is the context surrounding this content?
- What details might be significant that are easy to overlook?

## Principle 2: Simplicity First

Always reach for the simplest solution first. Simple code is easier to understand, debug, test, and extend. Resist the urge to build elaborate abstractions or optimize prematurely. Complexity should be earned — only introduce it when the simple solution demonstrably falls short.

Guidelines:
- Write code a junior engineer can understand
- Avoid premature abstraction and optimization
- Delete code whenever possible — less code is better code
- Favor boring, proven patterns over clever, novel ones

## Principle 3: Surgical Changes

Make the smallest possible change to achieve the goal. Each change should do exactly one thing, and do it well. Do not refactor unrelated code, fix unrelated bugs, or add "while I'm here" improvements. Precise, minimal changes reduce risk and make review straightforward.

Guidelines:
- One logical change per commit/PR
- Don't mix refactoring with feature work
- Leave the codebase cleaner than you found it — but only in the area you're touching
- If you see something broken that's out of scope, file an issue, don't fix it inline

## Principle 4: Goal-Driven Execution

Stay relentlessly focused on the goal. Do not chase shiny objects, explore interesting tangents, or get sidetracked by adjacent improvements. Every action should trace back to the acceptance criteria. If it's not required to meet the goal, it's a distraction.

Guidelines:
- Before every action, ask: "Does this directly advance the goal?"
- Track progress against acceptance criteria, not against interesting side quests
- Timebox exploration — if you need to research, set a limit and return to the goal
- Ship the minimum viable implementation, then iterate
