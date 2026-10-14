/** Which tracks make sound, given the tracks that are muted and the ones that are soloed. */
export function audibleTracks({ muted, solo }: { muted: boolean[]; solo: boolean[] }): boolean[] {
  const hasSolo = solo.some(Boolean)
  return muted.map((isMuted, track) => (hasSolo ? solo[track] === true : !isMuted))
}

/** The same list, cut or padded with false to `length` tracks. */
export function resized({ flags, length }: { flags: boolean[]; length: number }): boolean[] {
  return Array.from({ length }, (_, i) => flags[i] === true)
}
