/**
 * Each tab's session state, by tab id, for its label: whether the session is down (the label shows SuperXPuTTY's
 * disconnected badge). PageHost reports changes through TabArea.
 */
export const tabStates = $state<Record<string, { ended: boolean }>>({});
