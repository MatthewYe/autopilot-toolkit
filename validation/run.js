import { readFileSync, writeFileSync } from 'node:fs';
import { validateSkill } from './validate.js';

// 14 upstream skills: paths relative to project root
const projectRoot = new URL('..', import.meta.url).pathname.replace(/\/$/, '');

const skills = [
  // --- Upstream (14) ---
  { name: 'diagnose',     source: 'upstream', path: 'skills/upstream/skills/engineering/diagnosing-bugs/SKILL.md' },
  { name: 'grill-with-docs', source: 'upstream', path: 'skills/upstream/skills/engineering/grill-with-docs/SKILL.md' },
  { name: 'improve-codebase-architecture', source: 'upstream', path: 'skills/upstream/skills/engineering/improve-codebase-architecture/SKILL.md' },
  { name: 'prototype',    source: 'upstream', path: 'skills/upstream/skills/engineering/prototype/SKILL.md' },
  { name: 'setup-matt-pocock-skills', source: 'upstream', path: 'skills/upstream/skills/engineering/setup-matt-pocock-skills/SKILL.md' },
  { name: 'tdd',          source: 'upstream', path: 'skills/upstream/skills/engineering/tdd/SKILL.md' },
  { name: 'to-issues',    source: 'upstream', path: 'skills/upstream/skills/engineering/to-issues/SKILL.md' },
  { name: 'to-prd',       source: 'upstream', path: 'skills/upstream/skills/engineering/to-prd/SKILL.md' },
  { name: 'triage',       source: 'upstream', path: 'skills/upstream/skills/engineering/triage/SKILL.md' },
  { name: 'caveman',      source: 'upstream', path: 'skills/upstream/skills/productivity/grilling/SKILL.md' },
  { name: 'grill-me',     source: 'upstream', path: 'skills/upstream/skills/productivity/grill-me/SKILL.md' },
  { name: 'handoff',      source: 'upstream', path: 'skills/upstream/skills/productivity/handoff/SKILL.md' },
  { name: 'write-a-skill', source: 'upstream', path: 'skills/upstream/skills/productivity/writing-great-skills/SKILL.md' },
  { name: 'teach',        source: 'upstream', path: 'skills/upstream/skills/productivity/teach/SKILL.md' },

  // --- Autopilot (5) ---
  { name: 'audit-autopilot',        source: 'autopilot', path: 'skills/autopilot/audit-autopilot/SKILL.md' },
  { name: 'autopilot-implementer',  source: 'autopilot', path: 'skills/autopilot/autopilot-implementer/SKILL.md' },
  { name: 'autopilot-orchestrator', source: 'autopilot', path: 'skills/autopilot/autopilot-orchestrator/SKILL.md' },
  { name: 'autopilot-reviewer',     source: 'autopilot', path: 'skills/autopilot/autopilot-reviewer/SKILL.md' },
  { name: 'toolkit-selfcheck',      source: 'autopilot', path: 'skills/autopilot/toolkit-selfcheck/SKILL.md' },
];

let passed = 0;
let failed = 0;
const results = [];

for (const skill of skills) {
  const fullPath = skill.path.startsWith('/') ? skill.path : `${projectRoot}/${skill.path}`;
  let content;
  try {
    content = readFileSync(fullPath, 'utf-8');
  } catch (err) {
    results.push({
      name: skill.name,
      source: skill.source,
      file: fullPath,
      passed: false,
      issues: [`File not found: ${err.message}`],
    });
    failed++;
    continue;
  }

  const result = validateSkill(content);
  results.push({
    name: skill.name,
    source: skill.source,
    file: fullPath,
    passed: result.passed,
    issues: result.issues,
  });

  if (result.passed) passed++;
  else failed++;
}

// --- Fix documentation per failing issue type ---
function getFix(name, issues) {
  const fixes = [];
  for (const issue of issues) {
    if (issue.includes('disable-model-invocation')) {
      fixes.push(`Remove "disable-model-invocation: true" line from frontmatter`);
    }
    if (issue.includes('Missing required field: name')) {
      fixes.push(`Add "name: <skill-name>" to frontmatter`);
    }
    if (issue.includes('Missing required field: description')) {
      fixes.push(`Add "description: <description>" to frontmatter`);
    }
    if (issue.includes('Name')) {
      fixes.push(`Fix name to match ^[a-zA-Z0-9][a-zA-Z0-9._-]{0,63}$`);
    }
    if (issue.includes('runAs')) {
      fixes.push(`Set runAs to "inline" or "subagent"`);
    }
    if (issue.includes('allowed-tools')) {
      fixes.push(`Add "allowed-tools: <tool-list or TODO>" to frontmatter`);
    }
    if (issue.includes('File not found')) {
      fixes.push(`Ensure the SKILL.md file exists at the expected path or update .skill-lock.json path`);
    }
    if (issue.includes('delimiter')) {
      fixes.push(`Add missing "---" frontmatter delimiters`);
    }
  }
  if (fixes.length === 0 && issues.length > 0) {
    fixes.push(`Manual review needed: ${issues.join('; ')}`);
  }
  return fixes;
}

// --- Build report ---
const lines = [];

lines.push('='.repeat(70));
lines.push('FRONTMATTER VALIDATION REPORT — reasonix compatibility');
lines.push('='.repeat(70));
lines.push(`Date: ${new Date().toISOString()}`);
lines.push(`Total skills validated: ${skills.length} | Passed: ${passed} | Failed: ${failed}`);
lines.push('');

const passStr = (p) => (p ? 'PASS' : 'FAIL');

// Upstream section
lines.push('--- Upstream Skills (14) ---');
const upstreamResults = results.filter((r) => r.source === 'upstream');
const upstreamPassed = upstreamResults.filter((r) => r.passed).length;
const upstreamFailed = upstreamResults.filter((r) => !r.passed).length;
lines.push(`Passed: ${upstreamPassed} / Failed: ${upstreamFailed}`);
lines.push('');

for (const r of upstreamResults) {
  lines.push(`  [${passStr(r.passed)}] ${r.name}`);
  lines.push(`       File: ${r.file}`);
  for (const issue of r.issues) {
    lines.push(`       Issue: ${issue}`);
    for (const fix of getFix(r.name, [issue])) {
      lines.push(`       Fix:   ${fix}`);
    }
  }
  if (r.passed) {
    lines.push(`       ✓ All checks passed`);
  }
  lines.push('');
}

// Autopilot section
lines.push('--- Autopilot Skills (5) ---');
const autopilotResults = results.filter((r) => r.source === 'autopilot');
const autopilotPassed = autopilotResults.filter((r) => r.passed).length;
const autopilotFailed = autopilotResults.filter((r) => !r.passed).length;
lines.push(`Passed: ${autopilotPassed} / Failed: ${autopilotFailed}`);
lines.push('');

for (const r of autopilotResults) {
  lines.push(`  [${passStr(r.passed)}] ${r.name}`);
  lines.push(`       File: ${r.file}`);
  for (const issue of r.issues) {
    lines.push(`       Issue: ${issue}`);
    for (const fix of getFix(r.name, [issue])) {
      lines.push(`       Fix:   ${fix}`);
    }
  }
  if (r.passed) {
    // Show key frontmatter details for autopilot skills
    try {
      const content = readFileSync(r.file, 'utf-8');
      const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
      if (fmMatch) {
        const fm = fmMatch[1];
        const runAsMatch = fm.match(/^runAs:\s*(.+)$/m);
        const atMatch = fm.match(/^allowed-tools:\s*(.+)$/m);
        if (runAsMatch) lines.push(`       runAs: ${runAsMatch[1]}`);
        if (atMatch) lines.push(`       allowed-tools: ${atMatch[1]}`);
      }
    } catch (_) {}
  }
  lines.push('');
}

// Global checks
lines.push('='.repeat(70));
lines.push('GLOBAL CHECKS');
lines.push('='.repeat(70));
lines.push('');

// Check: 0 opencode-specific fields across all 18 skills
const opencodeFields = ['compatibility', 'mode', 'disable-model-invocation', 'permission', 'hidden', 'arguments'];
let totalOcFields = 0;
const ocFieldSkills = [];
for (const r of results) {
  for (const issue of r.issues) {
    // Extract field name from issue format: "OpenCode-specific field present: <field>"
    const match = issue.match(/OpenCode-specific field present: (\S+)/);
    if (match && opencodeFields.includes(match[1])) {
      totalOcFields++;
      if (!ocFieldSkills.includes(r.name)) ocFieldSkills.push(r.name);
    }
  }
}
lines.push(`Check: 0 opencode-specific fields across all 19 skills`);
lines.push(`Result: ${totalOcFields === 0 ? '✓ PASS' : `✗ FAIL — ${totalOcFields} opencode field(s) found in ${ocFieldSkills.length} skill(s): ${ocFieldSkills.join(', ')}`}`);
lines.push('');

// Check: all subagent skills have allowed-tools
const subagentSkills = [];
for (const r of results) {
  try {
    const content = readFileSync(r.file, 'utf-8');
    const fmMatch = content.match(/^---\n([\s\S]*?)\n---/);
    if (fmMatch) {
      const fm = fmMatch[1];
      if (/^runAs:\s*subagent/m.test(fm)) {
        const hasAllowedTools = /^allowed-tools:/m.test(fm);
        subagentSkills.push({ name: r.name, hasAllowedTools });
      }
    }
  } catch (_) {}
}

lines.push(`Check: All subagent skills have allowed-tools defined`);
if (subagentSkills.length === 0) {
  lines.push(`Result: ✓ PASS — No subagent skills found (or all non-subagent skills OK)`);
} else {
  const missingAt = subagentSkills.filter((s) => !s.hasAllowedTools);
  if (missingAt.length === 0) {
    lines.push(`Result: ✓ PASS — ${subagentSkills.length} subagent skill(s) all have allowed-tools:`);
    for (const s of subagentSkills) {
      const r = results.find((x) => x.name === s.name);
      if (r) {
        try {
          const content = readFileSync(r.file, 'utf-8');
          const atMatch = content.match(/^allowed-tools:\s*(.+)$/m);
          lines.push(`       ${s.name}: ${atMatch ? atMatch[1] : 'present'}`);
        } catch (_) {}
      }
    }
  } else {
    lines.push(`Result: ✗ FAIL — ${missingAt.length} subagent skill(s) missing allowed-tools: ${missingAt.map((s) => s.name).join(', ')}`);
  }
}
lines.push('');

// Overall
lines.push('='.repeat(70));
lines.push('OVERALL RESULT');
lines.push('='.repeat(70));
if (failed === 0) {
  lines.push('All skills PASS validation.');
} else {
  lines.push(`${failed} skill(s) FAIL validation. See individual entries above for issue details and fixes.`);
}

const report = lines.join('\n');
console.log(report);

// Save report
const reportPath = `${projectRoot}/validation/report.txt`;
writeFileSync(reportPath, report, 'utf-8');
console.log(`\nReport saved to: validation/report.txt`);
