"use client";

import { Terminal, Wrench } from "lucide-react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";

import type { SessionMessage } from "@/lib/types";

export function MessageView({ message }: { message: SessionMessage }) {
  if (message.is_tool_result) {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2">
          <Terminal className="mt-0.5 size-3 shrink-0 text-muted-foreground/60" />
          <pre className="min-w-0 flex-1 whitespace-pre-wrap break-words rounded-md border border-border/40 bg-muted/20 px-3 py-2 font-mono text-[11px] text-muted-foreground">
            {message.content}
          </pre>
        </div>
      </div>
    );
  }

  if (message.role === "tool") {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2">
          <Wrench className="mt-0.5 size-3 shrink-0 text-amber-600 dark:text-amber-400" />
          <div className="min-w-0 flex-1">
            <p className="text-[10px] font-mono font-medium uppercase tracking-wider text-amber-600 dark:text-amber-400">
              {message.tool_name ?? "tool"}
            </p>
            {message.content && (
              <pre className="mt-1 whitespace-pre-wrap break-words rounded-md border border-border/40 bg-muted/20 px-3 py-2 font-mono text-[11px] text-muted-foreground">
                {message.content}
              </pre>
            )}
          </div>
        </div>
      </div>
    );
  }

  if (message.role === "user") {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2.5">
          <span
            aria-hidden
            className="mt-0.5 select-none font-mono text-sm font-semibold leading-snug text-blue-600 dark:text-blue-400"
          >
            ›
          </span>
          <p className="min-w-0 flex-1 whitespace-pre-wrap break-words rounded-md bg-blue-500/10 px-3 py-2 text-xs text-foreground/90">
            {message.content}
          </p>
        </div>
      </div>
    );
  }

  if (message.role === "assistant") {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2.5">
          <span
            aria-hidden
            className="mt-0.5 select-none font-mono text-sm font-semibold leading-snug text-primary"
          >
            ✦
          </span>
          <div className="min-w-0 flex-1 rounded-md bg-primary/5 px-3 py-2 text-xs text-foreground/90 [&_p]:my-1.5 [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 [&_h1]:mt-3 [&_h1]:mb-1 [&_h1]:text-sm [&_h1]:font-semibold [&_h2]:mt-2.5 [&_h2]:mb-1 [&_h2]:text-xs [&_h2]:font-semibold [&_h3]:mt-2 [&_h3]:mb-1 [&_h3]:text-xs [&_h3]:font-semibold [&_ul]:my-1.5 [&_ul]:list-disc [&_ul]:pl-5 [&_ol]:my-1.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_li]:my-0.5 [&_a]:text-primary [&_a]:underline [&_strong]:font-semibold [&_em]:italic [&_blockquote]:border-l-2 [&_blockquote]:border-border/60 [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_code]:rounded [&_code]:bg-muted/60 [&_code]:px-1 [&_code]:py-0.5 [&_code]:font-mono [&_code]:text-[0.9em] [&_pre]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-border/40 [&_pre]:bg-muted/30 [&_pre]:p-3 [&_pre]:font-mono [&_pre]:text-[11px] [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_table]:my-2 [&_table]:w-full [&_table]:border-collapse [&_th]:border [&_th]:border-border/40 [&_th]:bg-muted/20 [&_th]:px-2 [&_th]:py-1 [&_th]:text-left [&_th]:font-semibold [&_td]:border [&_td]:border-border/40 [&_td]:px-2 [&_td]:py-1 [&_hr]:my-3 [&_hr]:border-border/40">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeSanitize]}
            >
              {message.content}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    );
  }

  // thinking marker the parser prefixes onto thinking-block content
  if (message.content.startsWith("[thinking]")) {
    return (
      <div className="px-4 py-2.5">
        <div className="flex items-start gap-2.5">
          <span
            aria-hidden
            className="mt-0.5 select-none font-mono text-sm font-semibold leading-snug text-muted-foreground/60"
          >
            ·
          </span>
          <p className="min-w-0 flex-1 whitespace-pre-wrap break-words rounded-md border border-dashed border-border/40 px-3 py-2 text-[11px] italic text-muted-foreground/80">
            {message.content.slice("[thinking]".length).trim()}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="px-4 py-2.5">
      <p className="whitespace-pre-wrap break-words text-xs text-foreground/90">
        {message.content}
      </p>
    </div>
  );
}