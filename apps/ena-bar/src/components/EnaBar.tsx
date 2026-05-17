"use client";

import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Mic,
  Search,
  X,
  Cpu,
  Sparkles,
  Terminal,
  Globe,
  FileCode,
  CheckCircle2,
  ChevronRight,
  Zap,
  Bot,
  Code2,
  BrainCircuit,
  ArrowUpRight,
} from "lucide-react";
import { cn } from "@/lib/utils";

// ─── Types ───────────────────────────────────────────────────────────────────

type BarState = "collapsed" | "idle" | "thinking" | "result";

interface ExecutionStep {
  icon: typeof Terminal;
  title: string;
  time: string;
  status: "done" | "active" | "pending" | "error";
}

// ─── Sample execution trace ───────────────────────────────────────────────────

const executionSteps: ExecutionStep[] = [
  { icon: Globe, title: "Browsing clinical trial data", time: "2.4s", status: "done" },
  { icon: Terminal, title: "Running python data validator", time: "1.8s", status: "done" },
  { icon: FileCode, title: "Generating summary report diff", time: "0.9s", status: "active" },
  { icon: Zap, title: "Cross-referencing knowledge graph", time: "0.4s", status: "pending" },
];

const capabilities = [
  { icon: Bot, label: "Research" },
  { icon: Code2, label: "Code" },
  { icon: BrainCircuit, label: "Memory" },
  { icon: Mic, label: "Voice" },
];

const suggestions = [
  "Open research workspace",
  "Summarize my inbox",
  "Deploy latest build",
  "Launch dev environment",
];

// ─── Spring configs ───────────────────────────────────────────────────────────

const springTight = { type: "spring" as const, stiffness: 300, damping: 25, mass: 0.8 };

// ─── Typewriter hook ──────────────────────────────────────────────────────────

function useTypewriter(text: string, speed = 30) {
  const [displayed, setDisplayed] = useState("");
  const [done, setDone] = useState(false);

  useEffect(() => {
    setDisplayed("");
    setDone(false);
    let i = 0;
    const interval = setInterval(() => {
      i++;
      setDisplayed(text.slice(0, i));
      if (i >= text.length) {
        clearInterval(interval);
        setDone(true);
      }
    }, speed);
    return () => clearInterval(interval);
  }, [text, speed]);

  return { displayed, done };
}

// ─── Tauri IPC helpers ────────────────────────────────────────────────────────

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  if (typeof window !== "undefined" && "__TAURI__" in window) {
    try {
      // Dynamic import to avoid crashes in non-Tauri environments.
      const { invoke } = await import("@tauri-apps/api/core");
      return invoke(cmd, args);
    } catch {
      // Fall through to simulated behavior.
    }
  }
  return null;
}

// ─── EnaBar Props ─────────────────────────────────────────────────────────────

interface EnaBarProps {
  isTauri?: boolean;
}

// ─── EnaBar Component ─────────────────────────────────────────────────────────

export function EnaBar({ isTauri = false }: EnaBarProps) {
  const [barState, setBarState] = useState<BarState>("collapsed");
  const [inputValue, setInputValue] = useState("");
  const [recentCommands, setRecentCommands] = useState<string[]>([]);
  const [feedVisible, setFeedVisible] = useState(true);
  const [hoverPos, setHoverPos] = useState({ x: 0.5, y: 0.5 });
  const inputRef = useRef<HTMLInputElement>(null);
  const barRef = useRef<HTMLDivElement>(null);

  // Focus input on expand.
  useEffect(() => {
    if (barState !== "collapsed" && inputRef.current) {
      inputRef.current.focus();
    }
  }, [barState]);

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (!inputValue.trim()) return;
      setRecentCommands((prev) => [inputValue, ...prev].slice(0, 5));
      setBarState("thinking");
      setFeedVisible(true);

      // Try real Tauri IPC; fall back to simulated transition.
      tauriInvoke("spawn_agent", {
        task: inputValue,
        capabilities: ["research", "code"],
      });

      setTimeout(() => setBarState("result"), 3000);
    },
    [inputValue]
  );

  const handleSuggestion = useCallback((suggestion: string) => {
    setInputValue(suggestion);
    setBarState("idle");
    inputRef.current?.focus();
  }, []);

  const handleMicClick = useCallback(() => {
    setBarState("thinking");
    setTimeout(() => {
      setInputValue("Voice input captured — processing speech...");
      setTimeout(() => setBarState("result"), 2000);
    }, 1500);
  }, []);

  const handleClose = useCallback(() => {
    setBarState("collapsed");
    setInputValue("");
    setFeedVisible(false);
  }, []);

  const handleExpand = useCallback(() => setBarState("idle"), []);
  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!barRef.current) return;
    const rect = barRef.current.getBoundingClientRect();
    setHoverPos({
      x: (e.clientX - rect.left) / rect.width,
      y: (e.clientY - rect.top) / rect.height,
    });
  }, []);

  // ── Render ──────────────────────────────────────────────────────────

  return (
    <div className="fixed bottom-6 left-0 right-0 z-50 flex justify-center px-4">
      <motion.div
        ref={barRef}
        layout
        initial={{ y: 120, opacity: 0 }}
        animate={{ y: 0, opacity: 1 }}
        transition={springTight}
        onMouseMove={handleMouseMove}
        className={cn(
          "group relative flex flex-col overflow-hidden transition-all duration-500",
          barState === "collapsed"
            ? "w-auto cursor-pointer rounded-full px-6 py-3"
            : "w-full max-w-2xl rounded-[2rem] shadow-2xl shadow-white/5"
        )}
        onClick={barState === "collapsed" ? handleExpand : undefined}
      >
        {/* Glass background */}
        <div
          className="absolute inset-0 rounded-[inherit] transition-colors duration-300"
          style={{
            background: barState === "collapsed"
              ? "rgba(255,255,255,0.03)"
              : "rgba(255,255,255,0.04)",
            backdropFilter: "blur(28px)",
            WebkitBackdropFilter: "blur(28px)",
            border: "1px solid rgba(255,255,255,0.08)",
          }}
        />

        {/* Cursor-following radial glow on expanded bar */}
        {barState !== "collapsed" && (
          <div
            className="absolute inset-0 rounded-[2rem] opacity-30 transition-opacity duration-700 pointer-events-none"
            style={{
              background: `radial-gradient(600px circle at ${hoverPos.x * 100}% ${hoverPos.y * 100}%, rgba(212,175,55,0.06), transparent 60%)`,
            }}
          />
        )}

        {/* Content (above glass) */}
        <div className="relative z-10">
          {/* ── Collapsed ── */}
          {barState === "collapsed" && (
            <div className="flex items-center gap-4">
              <div className="flex items-center gap-2.5">
                <div className="relative flex h-2.5 w-2.5">
                  <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-green-400 opacity-60" />
                  <span className="relative inline-flex h-full w-full rounded-full bg-green-500" />
                </div>
                <span className="text-xs font-semibold tracking-[0.15em] text-neutral-400">ENA</span>
              </div>
              <span className="text-sm text-neutral-600 select-none">Ask Ena anything...</span>
              <div className="flex items-center gap-1.5 ml-2">
                <kbd className="hidden rounded-md border border-white/[0.08] bg-white/[0.04] px-1.5 py-0.5 text-[10px] text-neutral-600 sm:inline-block">⌘</kbd>
                <kbd className="hidden rounded-md border border-white/[0.08] bg-white/[0.04] px-1.5 py-0.5 text-[10px] text-neutral-600 sm:inline-block">K</kbd>
              </div>
              <div className="ml-auto hidden sm:flex items-center gap-1.5 text-[10px] text-neutral-600">
                <span className="flex h-1.5 w-1.5 rounded-full bg-accent/60" />
                {isTauri ? "native" : "dev"}
              </div>
            </div>
          )}

          {/* ── Expanded ── */}
          {barState !== "collapsed" && (
            <>
              {/* Header */}
              <div className="flex items-center justify-between border-b border-white/[0.06] px-6 py-3">
                <div className="flex items-center gap-2.5">
                  <div className="flex h-5 w-5 items-center justify-center">
                    <Cpu className="h-3.5 w-3.5 text-accent" />
                  </div>
                  <span className="text-[10px] font-medium tracking-[0.15em] text-neutral-500">
                    ENA REASONING ENGINE
                  </span>
                  <span className="rounded-full bg-accent/10 px-1.5 py-0.5 text-[8px] font-semibold text-accent">v1.0</span>
                </div>
                <div className="flex items-center gap-3">
                  <div className="hidden items-center gap-1.5 sm:flex">
                    <span className="flex h-1.5 w-1.5">
                      <span
                        className={cn(
                          "absolute inline-flex h-1.5 w-1.5 rounded-full",
                          barState === "thinking" ? "animate-ping bg-yellow-400" : "bg-green-500"
                        )}
                      />
                      <span
                        className={cn(
                          "relative inline-flex h-1.5 w-1.5 rounded-full",
                          barState === "thinking" ? "bg-yellow-400" : "bg-green-500"
                        )}
                      />
                    </span>
                    <span className="text-[10px] text-neutral-600">
                      {barState === "thinking" ? "PROCESSING" : "ACTIVE"}
                    </span>
                  </div>
                  <button onClick={handleClose} className="rounded-full p-1.5 transition-colors hover:bg-white/10">
                    <X className="h-3.5 w-3.5 text-neutral-500" />
                  </button>
                </div>
              </div>

              {/* Content */}
              <div className="min-h-[220px] px-6 py-5">
                <AnimatePresence mode="wait">
                  {barState === "idle" && (
                    <motion.div
                      key="idle"
                      initial={{ opacity: 0, y: 12 }}
                      animate={{ opacity: 1, y: 0 }}
                      exit={{ opacity: 0, y: -12 }}
                      transition={{ duration: 0.25 }}
                      className="space-y-6"
                    >
                      {/* Capability pills */}
                      <div className="flex flex-wrap justify-center gap-2">
                        {capabilities.map((cap, i) => (
                          <motion.div
                            key={cap.label}
                            initial={{ opacity: 0, scale: 0.9 }}
                            animate={{ opacity: 1, scale: 1, transition: { delay: 0.05 * i } }}
                            className="flex items-center gap-1.5 rounded-lg border border-white/[0.06] bg-white/[0.02] px-3 py-1.5 text-[11px] text-neutral-500"
                          >
                            <cap.icon className="h-3 w-3 text-accent/70" />
                            {cap.label}
                          </motion.div>
                        ))}
                      </div>

                      {/* Suggestions */}
                      <div className="flex flex-wrap justify-center gap-2">
                        {suggestions.map((suggestion, i) => (
                          <motion.button
                            key={suggestion}
                            initial={{ opacity: 0, y: 8 }}
                            animate={{ opacity: 1, y: 0, transition: { delay: 0.08 * i + 0.2 } }}
                            whileHover={{ scale: 1.02 }}
                            whileTap={{ scale: 0.98 }}
                            onClick={() => handleSuggestion(suggestion)}
                            className="group flex items-center gap-1.5 rounded-full border border-white/[0.06] bg-white/[0.02] px-4 py-2 text-xs text-neutral-500 transition-all hover:border-accent/20 hover:bg-accent/[0.04] hover:text-neutral-300"
                          >
                            <ChevronRight className="h-3 w-3 text-accent/0 transition-all group-hover:text-accent/60" />
                            {suggestion}
                          </motion.button>
                        ))}
                      </div>

                      {/* Recent commands */}
                      {recentCommands.length > 0 && (
                        <div className="space-y-1.5 pt-2">
                          <p className="text-[9px] font-medium uppercase tracking-[0.2em] text-neutral-600">Recent</p>
                          {recentCommands.map((cmd, i) => (
                            <motion.button
                              key={`${cmd}-${i}`}
                              initial={{ opacity: 0, x: -8 }}
                              animate={{ opacity: 1, x: 0, transition: { delay: 0.3 + 0.05 * i } }}
                              whileHover={{ x: 4 }}
                              onClick={() => handleSuggestion(cmd)}
                              className="flex w-full items-center gap-2.5 rounded-lg border border-transparent px-3 py-2 text-left text-sm text-neutral-500 transition-all hover:border-white/[0.06] hover:bg-white/[0.02] hover:text-neutral-300"
                            >
                              <Search className="h-3 w-3 shrink-0 text-neutral-600" />
                              <span className="truncate">{cmd}</span>
                            </motion.button>
                          ))}
                        </div>
                      )}
                    </motion.div>
                  )}

                  {barState === "thinking" && (
                    <motion.div
                      key="thinking"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ duration: 0.2 }}
                      className="flex flex-col items-center gap-6"
                    >
                      {/* Animated waveform */}
                      <div className="flex h-16 items-center justify-center gap-[3px]">
                        {Array.from({ length: 16 }).map((_, i) => (
                          <motion.div
                            key={i}
                            animate={{ height: [6, 16, 26, 36, 22, 12, 6] }}
                            transition={{ duration: 1.2, repeat: Infinity, delay: i * 0.06, ease: "easeInOut" }}
                            className="w-[3px] rounded-full"
                            style={{
                              background: i % 3 === 0
                                ? "linear-gradient(to top, #d4af37, #f5e6a3)"
                                : "linear-gradient(to top, rgba(255,255,255,0.15), rgba(255,255,255,0.4))",
                            }}
                          />
                        ))}
                      </div>

                      <div className="space-y-1.5 text-center">
                        <p className="text-sm font-medium text-neutral-200">Synthesizing response</p>
                        <p className="text-xs text-neutral-500">
                          Orchestrating agents · Retrieving context · Generating output
                        </p>
                      </div>

                      <ExecutionFeed steps={executionSteps} mini />
                    </motion.div>
                  )}

                  {barState === "result" && (
                    <ResultView
                      inputValue={inputValue}
                      feedVisible={feedVisible}
                      onNewCommand={() => setBarState("idle")}
                      onToggleFeed={() => setFeedVisible(!feedVisible)}
                    />
                  )}
                </AnimatePresence>
              </div>

              {/* Input bar */}
              <form
                onSubmit={handleSubmit}
                className={cn(
                  "relative flex items-center gap-2 border-t border-white/[0.06] px-4 py-3",
                  barState === "thinking" && "pointer-events-none opacity-40"
                )}
              >
                <div className="relative flex-1">
                  <Search className="absolute left-4 top-1/2 h-4 w-4 -translate-y-1/2 text-neutral-600" />
                  <input
                    ref={inputRef}
                    type="text"
                    placeholder="Ask Ena anything..."
                    value={inputValue}
                    onChange={(e) => {
                      setInputValue(e.target.value);
                      if (barState !== "idle") setBarState("idle");
                    }}
                    className="h-12 w-full rounded-full border border-white/[0.06] bg-white/[0.02] pl-11 pr-4 text-sm text-white outline-none transition-all placeholder:text-neutral-600 focus:border-accent/20 focus:bg-white/[0.04] focus:ring-1 focus:ring-accent/10"
                  />
                </div>
                <motion.button
                  whileHover={{ scale: 1.04 }}
                  whileTap={{ scale: 0.94 }}
                  type="button"
                  onClick={handleMicClick}
                  className="relative flex h-12 w-12 shrink-0 items-center justify-center rounded-full bg-accent text-black shadow-lg transition-all hover:shadow-accent/30 hover:shadow-xl"
                >
                  <Mic className="h-5 w-5" />
                </motion.button>
              </form>
            </>
          )}
        </div>
      </motion.div>

      {/* Dim overlay behind expanded bar */}
      {barState !== "collapsed" && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.3 }}
          className="fixed inset-0 -z-10 bg-gradient-to-t from-black/70 via-black/20 to-transparent pointer-events-none"
        />
      )}
    </div>
  );
}

// ─── ExecutionFeed ────────────────────────────────────────────────────────────

function ExecutionFeed({ steps, mini = false }: { steps: ExecutionStep[]; mini?: boolean }) {
  return (
    <div className={cn("w-full rounded-xl border border-white/[0.06] bg-black/30 overflow-hidden", !mini && "shadow-lg")}>
      <div className="flex items-center gap-2 border-b border-white/[0.04] px-4 py-2">
        <div className="flex gap-1.5">
          <div className="h-2 w-2 rounded-full bg-neutral-700" />
          <div className="h-2 w-2 rounded-full bg-neutral-700" />
          <div className="h-2 w-2 rounded-full bg-neutral-700" />
        </div>
        <span className="ml-2 text-[9px] font-mono uppercase tracking-[0.15em] text-neutral-600">Live Execution Trace</span>
        <span className="ml-auto text-[9px] font-mono text-neutral-700">
          {steps.filter((s) => s.status === "done").length}/{steps.length} done
        </span>
      </div>
      <div className={cn("space-y-2", mini ? "p-3" : "p-4")}>
        {steps.map((step, idx) => (
          <motion.div
            key={step.title}
            initial={{ opacity: 0, x: -12 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ delay: idx * 0.12, duration: 0.35 }}
            className="flex items-start gap-2.5"
          >
            <div
              className={cn(
                "mt-0.5 flex shrink-0 items-center justify-center rounded-lg transition-colors duration-500",
                mini ? "h-6 w-6" : "h-7 w-7",
                step.status === "done" && "bg-white/[0.03] text-neutral-500",
                step.status === "active" && "bg-accent/10 text-accent",
                step.status === "pending" && "bg-white/[0.01] text-neutral-600"
              )}
            >
              <step.icon className={mini ? "h-3 w-3" : "h-3.5 w-3.5"} />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between gap-2">
                <span
                  className={cn(
                    "font-medium truncate",
                    mini ? "text-[11px]" : "text-xs",
                    step.status === "done" && "text-neutral-400",
                    step.status === "active" && "text-white",
                    step.status === "pending" && "text-neutral-600"
                  )}
                >
                  {step.title}
                </span>
                <span className="shrink-0 text-[9px] font-mono text-neutral-600">{step.time}</span>
              </div>
              {(step.status === "done" || step.status === "active") && (
                <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-white/[0.04]">
                  <motion.div
                    initial={{ width: 0 }}
                    animate={{ width: step.status === "done" ? "100%" : "60%" }}
                    transition={{ duration: 0.8, ease: "easeOut" }}
                    className={cn("h-full rounded-full", step.status === "done" && "bg-white/10", step.status === "active" && "bg-accent/50")}
                  />
                </div>
              )}
            </div>
            {step.status === "done" && <CheckCircle2 className={cn("shrink-0 text-neutral-600", mini ? "mt-0 h-3 w-3" : "mt-0.5 h-3.5 w-3.5")} />}
            {step.status === "active" && (
              <span className="mt-0.5 flex h-2 w-2 shrink-0">
                <span className="absolute inline-flex h-2 w-2 animate-ping rounded-full bg-accent opacity-60" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-accent" />
              </span>
            )}
          </motion.div>
        ))}
      </div>
    </div>
  );
}

// ─── ResultView ───────────────────────────────────────────────────────────────

function ResultView({
  inputValue,
  feedVisible,
  onNewCommand,
  onToggleFeed,
}: {
  inputValue: string;
  feedVisible: boolean;
  onNewCommand: () => void;
  onToggleFeed: () => void;
}) {
  const responseText = useMemo(() => {
    return `I've registered your request for "${inputValue}". The capability to execute this autonomous workflow is currently being synthesized across the agent network and will be available in the next kernel update.`;
  }, [inputValue]);

  const { displayed, done } = useTypewriter(responseText, 18);

  return (
    <motion.div
      key="result"
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -12 }}
      transition={{ duration: 0.3 }}
      className="space-y-5"
    >
      <div className="flex items-start gap-3.5">
        <motion.div
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          transition={{ type: "spring", stiffness: 300, damping: 15, delay: 0.1 }}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-accent/15"
        >
          <Sparkles className="h-[18px] w-[18px] text-accent" />
        </motion.div>
        <div className="space-y-2.5 pt-0.5">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-white">Ena Intelligence</span>
            <span className="rounded-full bg-white/[0.04] px-2 py-0.5 text-[9px] font-mono text-neutral-500">v1.0</span>
          </div>
          <p className="text-sm leading-relaxed text-neutral-300">
            {displayed}
            {!done && (
              <motion.span
                animate={{ opacity: [1, 0] }}
                transition={{ duration: 0.5, repeat: Infinity }}
                className="ml-0.5 inline-block h-3.5 w-[2px] bg-accent/70"
              />
            )}
          </p>
          {done && (
            <motion.div
              initial={{ width: 0 }}
              animate={{ width: "100%" }}
              transition={{ duration: 0.8, ease: "easeOut" }}
              className="h-px max-w-[120px] bg-gradient-to-r from-accent/40 to-transparent"
            />
          )}
        </div>
      </div>

      {feedVisible && (
        <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.2 }}>
          <ExecutionFeed steps={executionSteps} />
        </motion.div>
      )}

      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 1 }}
        className="flex items-center justify-between pt-1"
      >
        <button
          onClick={onNewCommand}
          className="group flex items-center gap-1 text-xs text-neutral-600 transition-colors hover:text-neutral-400"
        >
          <ArrowUpRight className="h-3 w-3 transition-transform group-hover:-translate-x-0.5" />
          New command
        </button>
        <button
          onClick={onToggleFeed}
          className="text-xs text-neutral-600 transition-colors hover:text-neutral-400"
        >
          {feedVisible ? "Hide trace" : "Show trace"}
        </button>
      </motion.div>
    </motion.div>
  );
}
