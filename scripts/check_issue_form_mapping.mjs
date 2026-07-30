#!/usr/bin/env node
// check_issue_form_mapping.mjs - Guard against .github/ISSUE_TEMPLATE/issue.yml and
// .github/workflows/issue-labels.yml silently drifting apart.
//
// The workflow maps the form's field `id:`s to label prefixes by name (PREFIX_FIELDS,
// plus the special-cased `epic` checkbox). If someone renames a field in the form
// without updating the workflow, labeling breaks silently — the field just stops
// producing labels, with no error, only occasionally-missing labels weeks later.
//
// Deliberately dependency-free: no js-yaml (not a declared dependency of this repo,
// only present transitively — relying on that would be its own drift risk). The `id:`
// fields we need have a fixed, simple shape, so a targeted regex scan is more honest
// than a full YAML parse we don't actually need.
//
// Exit codes: 0 = in sync, 1 = drifted (or files unreadable).

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const FORM_PATH = join(ROOT, '.github/ISSUE_TEMPLATE/issue.yml');
const WORKFLOW_PATH = join(ROOT, '.github/workflows/issue-labels.yml');

function extractFormFieldIds(yamlText) {
  // Matches "  id: component" style lines under body: list items. All current fields
  // sit at 4-space indent directly under a "- type: ..." block; that's the only shape
  // this file actually uses, so we don't need general YAML nesting awareness.
  const ids = [];
  for (const line of yamlText.split('\n')) {
    const m = line.match(/^\s{4}id:\s*(\S+)\s*$/);
    if (m) ids.push(m[1]);
  }
  return ids;
}

function extractWorkflowFieldIds(workflowYamlText) {
  // Pull the PREFIX_FIELDS object literal's keys out of the embedded script, plus the
  // hardcoded 'epic' special case. Looking for the literal source text, not evaluating
  // it, keeps this script simple and safe.
  const blockMatch = workflowYamlText.match(/const PREFIX_FIELDS = \{([\s\S]*?)\};/);
  if (!blockMatch) {
    throw new Error('Could not find PREFIX_FIELDS object in issue-labels.yml — has the script been restructured?');
  }
  const keys = [...blockMatch[1].matchAll(/^\s*(\w+):/gm)].map(m => m[1]);

  const hasEpicHandling = /toApply\.add\('type: epic'\)/.test(workflowYamlText);
  if (hasEpicHandling) keys.push('epic');

  return keys;
}

const formIds = extractFormFieldIds(readFileSync(FORM_PATH, 'utf8'));
const workflowIds = extractWorkflowFieldIds(readFileSync(WORKFLOW_PATH, 'utf8'));

// 'description' is the free-text field — deliberately has no label mapping.
const mappableFormIds = formIds.filter(id => id !== 'description');

const missingFromWorkflow = mappableFormIds.filter(id => !workflowIds.includes(id));
const staleInWorkflow = workflowIds.filter(id => !mappableFormIds.includes(id));

if (missingFromWorkflow.length || staleInWorkflow.length) {
  console.error('Issue form <-> issue-labels.yml mapping has drifted:\n');
  if (missingFromWorkflow.length) {
    console.error(`  Field(s) in issue.yml with no mapping in issue-labels.yml: ${missingFromWorkflow.join(', ')}`);
    console.error(`  -> these fields will silently produce no labels.\n`);
  }
  if (staleInWorkflow.length) {
    console.error(`  Field(s) issue-labels.yml expects that no longer exist in issue.yml: ${staleInWorkflow.join(', ')}`);
    console.error(`  -> dead mapping entries; harmless but worth cleaning up.\n`);
  }
  console.error(`issue.yml field ids (excl. description): ${mappableFormIds.join(', ')}`);
  console.error(`issue-labels.yml known ids:               ${workflowIds.join(', ')}`);
  process.exit(1);
}

console.log(`OK — issue.yml and issue-labels.yml agree on: ${mappableFormIds.join(', ')}`);
