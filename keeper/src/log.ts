type Fields = Record<string, unknown>;

function line(level: "info" | "warn" | "error", msg: string, fields?: Fields) {
  const out = JSON.stringify({ ts: new Date().toISOString(), level, msg, ...fields }, (_k, v) =>
    typeof v === "bigint" ? v.toString() : v,
  );
  (level === "error" ? process.stderr : process.stdout).write(out + "\n");
}

export const log = {
  info: (msg: string, fields?: Fields) => line("info", msg, fields),
  warn: (msg: string, fields?: Fields) => line("warn", msg, fields),
  error: (msg: string, fields?: Fields) => line("error", msg, fields),
};
