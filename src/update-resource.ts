export async function releaseUpdateResource(
  update: { close(): Promise<void> } | null,
): Promise<void> {
  try {
    await update?.close();
  } catch (error) {
    // A discarded resource must not prevent checking or installing its replacement.
    console.warn("Could not release the previous update resource:", error);
  }
}
