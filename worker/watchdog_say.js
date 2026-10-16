// SPDX-License-Identifier: AGPL-3.0-or-later
// WHAT THE WATCHDOG WRITES DOWN (worker/factory.js `watchdog`, `watchdog_said`):
// a level, what happened, whether it is healthy and what to do, then the raw
// finding. Chinese, because the owner may read it; the raw finding stays English.

/// 🔴 needs someone now, 🟡 watch whether it clears, 🟢 progress only.
const LEVEL = { act: "🔴 要处理", watch: "🟡 留意", info: "🟢 进度" };

const hoursAgo = (at, now) => Math.max(0, (now - Date.parse(at)) / 3_600_000);
const fmtHours = (h) => (h < 1 ? `${Math.round(h * 60)} 分钟` : `${h < 10 ? h.toFixed(1) : Math.round(h)} 小时`);

/// A FINDING AS TEXT. Every kind `audit` and `watchdog` produce has a line here;
/// one without falls back to the raw finding under 🟡, never to silence.
export function watchdogText(f, now = Date.now()) {
  const [level, title, body] = describe(f, now);
  return `${LEVEL[level]} · ${title}\n${body}\n\`\`\`\n${f.kind} ${JSON.stringify(f)}\n\`\`\``;
}

const cellName = (ruler) => ruler.replace("__", " · ");

function describe(f, now) {
  if (f.kind.startsWith("stalled:")) {
    return ["act", `${f.producer} 类任务派不出去`,
      `${f.questions} 个已过期的 ${f.producer} 题半小时内一个回答都没有，同时有 ${f.witnesses_active} 台机器在干别的活（最早到期已 ${fmtHours(hoursAgo(f.oldest_due, now))}）。` +
      `\n异常：这类题没被派给任何人。叫 agent 查派发。`];
  }
  if (f.kind === "refused_rows") {
    const ids = f.sample.map((s) => s.question).join("，");
    return ["act", "客户端引擎拒绝计算的题已撤回",
      `${f.questions} 个题被多个不同的人报告"引擎拒绝"（/api/board/order 返回 ok:false），已自动撤回。例：${ids}。` +
      `\n异常：有不合准入的配装进了队列。叫 agent 查是哪条排队路径放进来的。`];
  }
  if (f.kind === "nondeterministic") {
    return ["act", "结果不确定的题",
      `${f.questions} 个题：要么几台机器各给一个不同答案，要么 24 小时内官方结算后发现没有志愿者答对。` +
      `\n异常：引擎或客户端可能有不确定性。叫 agent 查。`];
  }
  if (f.kind === "unsettled") {
    return ["watch", "争议/抽查题等官方结算",
      `${f.questions} 个题最后一个回答之后已超过 2 小时还没结算。settle 每小时跑一次（GitHub 常晚开半小时）。` +
      `\n偶尔一轮可能是 settle 晚开；连续几轮都在，就看 GitHub Actions 里 settle 有没有跑、有没有领到题。`];
  }
  if (f.kind === "overdue") {
    const oldest = f.producers.reduce((m, p) => Math.max(m, hoursAgo(p.oldest_due, now)), 0);
    const by = f.producers.map((p) => `${p.producer} ${p.questions}`).join("，");
    return ["watch", "过了截止时间还没做的题",
      `${f.questions} 个题已过截止时间（${by}），最久的超期 ${fmtHours(oldest)}。` +
      `\n高峰时偶尔出现可以等；连续几轮还在、数字没降，说明志愿者和官方后备都跟不上，要加算力或叫 agent 查派发。`];
  }
  if (f.kind === "disagreeing") {
    const top = f.owners.map((o) => `${o.owner ?? "未认领"} ${o.differ}/${o.answers}（${o.devices} 台）`).join("，");
    return ["act", "客户端与事实不符",
      `过去 1 小时引擎 ${f.engine} 上 ${f.answers} 个回答里 ${f.differ} 个不一致（${Math.round((100 * f.differ) / f.answers)}%）。最多的：${top}。` +
      `\n异常：可能是新发布或某个客户端改了数字。只集中在一个 owner 多半是他的客户端；分散在很多人说明是发布。叫 agent 查。`];
  }
  if (f.kind === "cells_filling") {
    const still = f.cells.filter((c) => c.owed > 0 && !c.recent);
    const lines = f.cells.map((c) => {
      const done = c.measured + c.owed, pct = done ? Math.round((100 * c.measured) / done) : 100;
      const eta = c.recent ? `约 ${fmtHours((c.owed / c.recent) * c.recent_hours)}` : "没在动";
      return `${cellName(c.ruler)}  ${c.measured}/${done}  ${pct}%  +${c.recent}/${c.recent_hours}h  ${eta}`;
    });
    const verdict = still.length
      ? `异常：${still.map((c) => cellName(c.ruler)).join("、")} ${f.cells[0].recent_hours} 小时一行都没算出来。叫 agent 查这个场景的行有没有派出去。`
      : `正常：每格都在动。剩余会随每轮排队增加，预计时间只按已排队的算。某格 ${f.cells[0].recent_hours} 小时没动会升为 🔴。`;
    return [still.length ? "act" : "info", "新场景填充中", `\`\`\`\n格子  已算/总数  进度  最近  预计剩余\n${lines.join("\n")}\n\`\`\`\n${verdict}`];
  }
  if (f.kind.startsWith("idle:")) return idleSaid(f, now);
  return ["watch", f.kind, "看门狗没有这一类的说明，原始数据见下。"];
}

/// HOW MANY IDLE DEVICES ARE LISTED BY NAME; the rest are counted.
const IDLE_LISTED = 10;
function idleSaid(f, now) {
  const line = (x) => {
    const last = x.last_answer ? `${fmtHours(hoursAgo(x.last_answer, now))}前` : "6 小时内没有";
    return `${x.device.slice(0, 8)} · ${x.label || "未认领"} · 引擎 ${x.engine} 协议 ${x.protocol} · ${x.lanes ?? "?"} 核 · 近 1 小时请求 ${x.asks} 次 · 上次交结果 ${last}`;
  };
  const list = f.devices.slice(0, IDLE_LISTED).map(line).join("\n")
    + (f.devices.length > IDLE_LISTED ? `\n……另有 ${f.devices.length - IDLE_LISTED} 台` : "");
  const head = `在线 ${f.online} 台里有 ${f.devices.length} 台`;
  if (f.kind === "idle:old_engine") {
    return ["watch", "设备停在旧引擎上，拿不到单",
      `${head}还在用旧引擎来要单，服务端只认 ${f.served}，给不了它们活。\n\`\`\`\n${list}\n\`\`\`\n` +
      "网页闲置 10 分钟会自己刷新，桌面版闲置 10 分钟会自己重启进新版；协议 7 或更老、几天没交过结果的，多半是很久没碰的老页面、更新失败的桌面版或第三方客户端。认领了的可以找设备主人刷新或更新。"];
  }
  if (f.kind === "idle:asking_loop") {
    return ["watch", "设备频繁请求却不接单",
      `${head}每小时请求超过 ${f.loop_per_hour} 次（官方页面没单时最多每分钟一次），30 分钟内一个结果都没交。\n\`\`\`\n${list}\n\`\`\`\n` +
      "官方页面不会这样，多半是第三方客户端的毛病（比如一直发\"不要单\"的请求）。联系客户端作者。"];
  }
  const many = f.devices.length * 10 >= f.online;
  return [many ? "act" : "watch", "设备在线但没在算",
    `${head}用的是当前引擎、在来要单，但 30 分钟内手里没单、也没交过结果。\n\`\`\`\n${list}\n\`\`\`\n` +
    (many ? "异常：占在线设备一成以上，像是派单出了问题。叫 agent 查派单。"
      : "零星几台多半是那台机器自己的问题（浏览器计算线程起不来、拿到单又交回）。持续出现或变多就叫 agent 查。")];
}
