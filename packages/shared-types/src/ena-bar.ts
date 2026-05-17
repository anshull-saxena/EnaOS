// EnaOS — Ena Bar Shared Types

/** Interaction states for the Ena Bar */
export type EnaBarState = "collapsed" | "idle" | "thinking" | "result";

/** System status indicator */
export type SystemStatus = "ready" | "processing" | "idle" | "error";

/** Agent execution item in the feed */
export interface ExecutionStep {
  id: string;
  type: "search" | "analysis" | "code" | "automation" | "voice" | "system";
  title: string;
  description?: string;
  timestamp: number;
  duration?: string;
  status: "pending" | "active" | "done" | "error";
}

/** Ena Bar command input */
export interface EnaCommand {
  text: string;
  timestamp: number;
}

/** Ena Bar execution feed item displayed in UI */
export interface ExecutionFeedItem {
  id: string;
  icon: string;
  title: string;
  time: string;
  status: "pending" | "active" | "done" | "error";
  steps?: ExecutionStep[];
}

/** Quick system context for the bar */
export interface SystemContext {
  status: SystemStatus;
  activeModel?: string;
  memoryUsage?: number;
  uptime?: string;
}
