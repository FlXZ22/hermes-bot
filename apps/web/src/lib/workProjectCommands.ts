import { HermesApiError, type HermesClient } from "@grok-bot/hermes-client";
import { parse as parseJsonc, printParseErrorCode, type ParseError } from "jsonc-parser";
import { parse as parseYaml } from "yaml";

interface ProjectCommand {
  template: string;
  description?: string;
  agent?: string;
  model?: string;
  subagent?: boolean;
}

const CONFIG_PATHS = ["opencode.json", "opencode.jsonc", ".opencode/opencode.json", ".opencode/opencode.jsonc"];
const COMMAND_DIRECTORIES = [".opencode/command", ".opencode/commands"];
const POSITION = /\$([1-9]\d*)/g;
const ARGUMENT = /\$(?:ARGUMENTS|([1-9]\d*))/g;
const SHELL_IN_TEMPLATE = /!`[^`]+`/;

function commandName(name: string): boolean {
  return name.length <= 240 && name.split("/").every(part =>
    part && part !== "." && part !== ".." && !/[\s\\`\u0000-\u001f]/.test(part));
}

function record(value: unknown): Record<string, unknown> | undefined {
  if (value && typeof value === "object" && !Array.isArray(value)) return value as Record<string, unknown>;
}

function readDefinition(value: unknown): ProjectCommand | undefined {
  const spec = record(value);
  if (typeof spec?.template !== "string" || !spec.template.trim()) return;
  const subagent = spec.subagent ?? spec.subtask;
  return {
    template: spec.template.trim(),
    description: typeof spec.description === "string" ? spec.description : undefined,
    agent: typeof spec.agent === "string" ? spec.agent : undefined,
    model: typeof spec.model === "string" ? spec.model : undefined,
    subagent: typeof subagent === "boolean" ? subagent : undefined,
  };
}

async function ifPresent<T>(read: Promise<T>): Promise<T | undefined> {
  try {
    return await read;
  } catch (error) {
    if (!(error instanceof HermesApiError && error.status === 404)) throw error;
  }
}

async function configCommands(client: HermesClient, cwd: string): Promise<Map<string, ProjectCommand>> {
  const files = await Promise.all(CONFIG_PATHS.map(path => ifPresent(client.readFile(`${cwd}/${path}`))));
  const commands = new Map<string, ProjectCommand>();
  files.forEach((contents, index) => {
    if (!contents) return;
    const errors: ParseError[] = [];
    const document = record(parseJsonc(contents, errors, { allowTrailingComma: true }));
    if (errors.length) throw new Error(`${CONFIG_PATHS[index]} contains invalid JSONC (${printParseErrorCode(errors[0].error)}).`);
    for (const [name, value] of Object.entries({ ...record(document?.command), ...record(document?.commands) })) {
      const spec = readDefinition(value);
      if (commandName(name) && spec) commands.set(name, spec);
    }
  });
  return commands;
}

function readCommandMarkdown(contents: string): ProjectCommand | undefined {
  const header = /^\uFEFF?---[^\S\n]*\r?\n((?:[\s\S]*?\r?\n)?)---[^\S\n]*(?:\r?\n|$)/.exec(contents);
  const metadata = header ? record(parseYaml(header[1], { maxAliasCount: 0 })) : undefined;
  return readDefinition({ ...metadata, template: (header ? contents.slice(header[0].length) : contents) });
}

async function projectMarkdownNames(client: HermesClient, cwd: string, directory: string): Promise<string[]> {
  const folders = [""];
  const names: string[] = [];
  for (let index = 0; index < folders.length; index++) {
    if (index === 128) throw new Error("The project contains more than 128 command folders. Invoke a command directly by name.");
    const folder = folders[index];
    const entries = await ifPresent(client.listFiles(`${cwd}/${directory}${folder ? `/${folder}` : ""}`)) ?? [];
    for (const entry of entries) {
      const path = folder ? `${folder}/${entry.name}` : entry.name;
      if (entry.name.includes("/") || !commandName(path)) continue;
      if (entry.is_dir) folders.push(path);
      else if (entry.name.endsWith(".md") && commandName(path.slice(0, -3))) names.push(path.slice(0, -3));
    }
  }
  return names;
}

function expandTemplate(template: string, input: string): string {
  const args = (input.match(/(?:\[Image\s+\d+\]|"[^"]*"|'[^']*'|[^\s"']+)/gi) ?? [])
    .map(argument => argument.replace(/^["']|["']$/g, ""));
  const last = Array.from(template.matchAll(POSITION)).reduce((max, match) => Math.max(max, Number(match[1])), 0);
  const text = template.replace(ARGUMENT, (match, index: string | undefined) => {
    if (match === "$ARGUMENTS") return input;
    const position = Number(index);
    return position === last ? args.slice(position - 1).join(" ") : args[position - 1] ?? "";
  });
  return (!last && !template.includes("$ARGUMENTS") && input ? `${text}\n\n${input}` : text).trim();
}

export async function dispatchProjectCommand(client: HermesClient, path: string, input: string): Promise<Record<string, unknown>> {
  const cwd = path.replace(/\/$/, "");
  if (input === "/oc") {
    const [configured, ...directories] = await Promise.all([
      configCommands(client, cwd), ...COMMAND_DIRECTORIES.map(directory => projectMarkdownNames(client, cwd, directory)),
    ]);
    const names = [...new Set([...configured.keys(), ...directories.flat()])].sort();
    const listing = names.map(name => {
      const description = configured.get(name)?.description?.replace(/\s+/g, " ").trim();
      return `- \`/oc/${name}\`${description ? ` — ${description}` : ""}`;
    });
    return { type: "exec", output: listing.join("\n") || "No OpenCode commands found in this workspace." };
  }
  const [name] = input.slice(4).split(/\s/, 1);
  if (!commandName(name)) throw new Error("Enter a project command name after /oc/.");
  const args = input.slice(name.length + 4).trimStart();
  const [configured, ...markdown] = await Promise.all([
    configCommands(client, cwd),
    ...COMMAND_DIRECTORIES.map(directory => ifPresent(client.readFile(`${cwd}/${directory}/${name}.md`))),
  ]);
  const contents = markdown.at(-1) || markdown[0];
  const spec = (contents ? readCommandMarkdown(contents) : undefined) ?? configured.get(name);
  if (!spec) throw new Error(`Project command ${name} was not found. Use /oc to list workspace commands.`);
  let message = expandTemplate(spec.template, args);
  if (SHELL_IN_TEMPLATE.test(message)) {
    message = `Before answering, use your terminal tool in the active workspace to collect the requested command outputs shown in !\`…\` blocks. Use the outputs as context for this request.\n\n${message}`;
  }
  const overrides = [spec.agent && `agent ${spec.agent}`, spec.model && `model ${spec.model}`, spec.subagent && "background subagent"].filter(Boolean);
  return {
    type: "send", message,
    notice: overrides.length ? `Using this Work session's agent and model for ${name}. OpenCode routing in the template: ${overrides.join(", ")}.` : undefined,
  };
}
