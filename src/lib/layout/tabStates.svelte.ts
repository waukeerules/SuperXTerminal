/**
 * Each tab's state, by tab id, for its label: whether the session is down (SuperXPuTTY's disconnected badge), and an
 * output alert from Watch for Activity or Watch for Silence. PageHost and TabArea report changes.
 */
export const tabStates = $state<Record<string, { ended?: boolean; alert?: "activity" | "silence" | null }>>({});
