/**
 * Each tab's state, by tab id, for its label: whether the session is still connecting (a yellow dot) or down
 * (SuperXPuTTY's disconnected badge), an
 * output alert from Watch for Activity or Watch for Silence, and its profile's tab colour (known once the session
 * starts, and again when the profile is saved). PageHost and TabArea report changes.
 */
export const tabStates = $state<Record<string, { ended?: boolean; connecting?: boolean; alert?: "activity" | "silence" | null; tabColor?: string | null }>>({});
