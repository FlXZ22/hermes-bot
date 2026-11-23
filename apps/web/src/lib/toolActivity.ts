import type { HermesToolCall } from '@grok-bot/hermes-client';
import { classifyHermesActivity } from './botState';

const kinds: Record<string, { icon: string; active: string; past: string }> = {
  writing: { icon: 'pencil', active: 'Writing', past: 'Wrote' },
  reading: { icon: 'book-open', active: 'Reading', past: 'Read' },
  searching: { icon: 'search', active: 'Searching', past: 'Searched' },
  running: { icon: 'terminal', active: 'Running', past: 'Ran' },
  coding: { icon: 'code', active: 'Working on', past: 'Worked on' },
  computing: { icon: 'globe', active: 'Using', past: 'Used' },
  sending: { icon: 'send', active: 'Sending', past: 'Sent' },
};

export function describeTool(tool: HermesToolCall) {
  const state = classifyHermesActivity(tool.name);
  const kind = kinds[state] ?? { icon: 'wrench', active: 'Using', past: 'Used' };
  // Inspect only the small input object; never parse/highlight outputs until expanded.
  let args: Record<string, unknown> = {};
  if (typeof tool.arguments === 'object' && tool.arguments) args = tool.arguments;
  else if (typeof tool.arguments === 'string' && tool.arguments.length < 8192) {
    try { const parsed = JSON.parse(tool.arguments); if (parsed && typeof parsed === 'object') args = parsed; } catch { /* Partial streaming arguments. */ }
  }
  const detail = [args.command, args.cmd, args.path, args.file_path, args.filename, args.query, args.url, tool.preview]
    .find(value => typeof value === 'string' && value.trim()) as string | undefined;
  return { ...kind, state, label: tool.status === 'running' ? kind.active : kind.past,
    detail: (detail || tool.name).slice(0, 1024).replace(/\s+/g, ' ').slice(0, 240) };
}
