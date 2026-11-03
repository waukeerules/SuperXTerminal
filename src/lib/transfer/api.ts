/**
 * File transfer's calls to the core (sftp.rs): the connection, both sides' folders, and copying either way.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import type { Store } from "../sessions/api";

/** One file or folder in a pane */
export interface Entry {
  name: string;
  dir: boolean;
  link: boolean;
  size: number;
  /** Seconds since 1970; 0 when unknown */
  modified: number;
  /** The time as the device wrote it, when it isn't one (SCP's `dir`) */
  date: string;
  permissions: string;
  owner: string;
  group: string;
}

export interface Listing {
  path: string;
  entries: Entry[];
}

export interface Opened {
  id: number;
  home: string;
  remote: string;
  local: string;
  address: string;
  /** SCP instead of SFTP: listed with the device's `dir`, files copied one at a time, nothing else */
  scp: boolean;
}

export type Progress = { type: "total"; bytes: number; files: number } | { type: "done"; bytes: number; file: string };

export const openTransfer = (store: Store, id: string) => invoke<Opened>("sftp_open", { store, id });
export const closeTransfer = (id: number) => invoke("sftp_close", { id }).catch(() => {});
export const listRemote = (id: number, path: string) => invoke<Listing>("sftp_list", { id, path });
export const listLocal = (path: string) => invoke<Listing>("local_list", { path });
export const remoteMkdir = (id: number, path: string) => invoke("sftp_mkdir", { id, path });
export const remoteRename = (id: number, from: string, to: string) => invoke("sftp_rename", { id, from, to });
export const remoteDelete = (id: number, path: string) => invoke("sftp_delete", { id, path });
export const localMkdir = (path: string) => invoke("local_mkdir", { path });
export const localRename = (from: string, to: string) => invoke("local_rename", { from, to });
export const revealLocal = (path: string) => invoke("local_reveal", { path });
export const cancelTransfer = (transfer: number) => invoke("sftp_cancel", { transfer });

/**
 * Copy files and folders into a folder; resolves when done, throws with the reason (or "Cancelled"). `sizes`: a
 * download's sizes from the listing, for SCP's progress (it can't tell them)
 */
export function transfer(id: number, transferId: number, upload: boolean, sources: string[], target: string, onProgress: (p: Progress) => void, sizes?: number[]): Promise<void> {
  const channel = new Channel<Progress>();
  channel.onmessage = onProgress;
  return invoke("sftp_transfer", { id, transfer: transferId, upload, sources, sizes: sizes ?? null, target, onProgress: channel });
}

/** A path joined to a name, with the side's separator */
export function joinPath(dir: string, name: string): string {
  return dir.endsWith("/") ? `${dir}${name}` : `${dir}/${name}`;
}

/** The folder a path is in ("/" stays "/") */
export function parentPath(path: string): string {
  // a device's root (flash:, flash:/) has no parent
  if (/^[\w-]+:\/?$/.test(path)) return path;
  const trimmed = path.replace(/\/+$/, "");
  const at = trimmed.lastIndexOf("/");
  if (at < 0) return path;
  if (at === 0) return "/";
  // flash:/dir goes up to flash:/
  return trimmed[at - 1] === ":" ? trimmed.slice(0, at + 1) : trimmed.slice(0, at);
}

/** 1.2 MB */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/** 2026-03-02 11:59, in this Mac's time */
export function formatTime(seconds: number): string {
  if (!seconds) return "";
  const d = new Date(seconds * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}
