import type { HermesClient } from "@grok-bot/hermes-client";

export async function resolveWorkGitRoot(client: HermesClient, cwd: string) {
  const { worktrees } = await client.listGitWorktrees(cwd);
  return worktrees
    .map(tree => tree.path)
    .filter(path => cwd === path || cwd.startsWith(`${path.replace(/\/$/, "")}/`))
    .sort((a, b) => b.length - a.length)[0] ?? cwd;
}

export async function readWorkGitDiff(client: HermesClient, cwd: string, command: string): Promise<string> {
  const args = (command.slice(5).match(/"[^"]*"|'[^']*'|\S+/g) ?? []).map(arg => arg.replace(/^(['"])(.*)\1$/, "$2"));
  const options = new Set(["working", "unstaged", "staged", "--staged", "all", "--all", "--stat", "stat"]);
  const pathspecs = args.filter(arg => !options.has(arg)).map(arg => arg.replace(/^\.\//, ""));
  const mode = args.some(arg => arg === "staged" || arg === "--staged") ? "staged"
    : args.some(arg => arg === "all" || arg === "--all") ? "uncommitted" : "unstaged";
  const root = await resolveWorkGitRoot(client, cwd);
  const git = await client.getGitStatus(root);
  if (!git) return "The Work folder is outside a Git repository.";
  const files = git.files.filter(file => {
    const match = mode === "staged" ? file.staged : mode === "unstaged" ? file.unstaged || file.conflicted : true;
    return match && (!pathspecs.length || pathspecs.some(path => file.path === path || file.path.startsWith(`${path.replace(/\/$/, "")}/`)));
  });
  if (!files.length) return "No changes in this diff scope.";
  if (args.includes("--stat") || args.includes("stat")) {
    return files.map(file => `${file.untracked ? "New" : file.staged ? "Staged" : "Modified"}  ${file.path}`).join("\n");
  }

  let diff = "";
  const count = Math.min(24, files.length);
  for (let offset = 0; offset < count && diff.length < 48_000; offset += 6) {
    const next = await Promise.all(files.slice(offset, Math.min(count, offset + 6)).map(
      file => client.getGitFileDiff(root, file.path, mode)
    ));
    diff += next.map(file => file.diff).filter(Boolean).join("\n");
  }
  if (!diff) return "No file contents are available for this diff.";
  const truncated = count < files.length || diff.length > 48_000;
  return `\`\`\`\`diff\n${diff.slice(0, 48_000)}\n\`\`\`\`${truncated ? "\n\nDiff preview truncated." : ""}`;
}
