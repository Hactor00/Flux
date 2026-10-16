export function compareVersions(left, right) {
  const parse = (value) => {
    const match =
      typeof value === "string" && value.match(/^\d+(?:\.\d+){0,3}$/);
    return match ? match[0].split(".").map(Number) : null;
  };
  const a = parse(left);
  const b = parse(right);
  if (!a || !b) return null;
  for (let index = 0; index < Math.max(a.length, b.length); index++) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference) return Math.sign(difference);
  }
  return 0;
}
