// SPDX-License-Identifier: AGPL-3.0-or-later
// WHAT THE WATCHDOG SAYS TO THE OWNER (worker/factory.js `watchdog`): a level,
// what happened, whether it is healthy and what to do, then the raw finding.
// Chinese, because the owner is its one reader; the raw finding stays English.

/// 🔴 needs someone now, 🟡 watch whether it clears, 🟢 progress only.
const LEVEL = { act: "🔴 要处理", watch: "🟡 留意", info: "🟢 进度" };

const hoursAgo = (at, now) => Math.max(0, (now - Date.parse(at)) / 3_600_000);
const fmtHours = (h) => (h < 1 ? `${Math.round(h * 60)} 分钟` : `${h < 10 ? h.toFixed(1) : Math.round(h)} 小时`);

/// A FINDING AS TEXT. Every kind `audit` and `watchdog` produce has a line here;
/// one without falls back to the raw finding under 🟡, never to silence.
export function watchdogText(f, now = Date.now()) {
  const [level, title, body] = describe(f, now);
  // A RULER ID HAS `__`, which Discord reads as underline: the raw finding goes
  // in a code block, and ids in the prose are written with ` · `.
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
      `\n偶尔一次可能是 settle 晚开；6 小时后再报，就看 GitHub Actions 里 settle 有没有跑、有没有领到题。`];
  }
  if (f.kind === "overdue") {
    const oldest = f.producers.reduce((m, p) => Math.max(m, hoursAgo(p.oldest_due, now)), 0);
    const by = f.producers.map((p) => `${p.producer} ${p.questions}`).join("，");
    return ["watch", "过了截止时间还没做的题",
      `${f.questions} 个题已过截止时间（${by}），最久的超期 ${fmtHours(oldest)}。` +
      `\n高峰时偶尔出现可以等；6 小时后还在、数字没降，说明志愿者和官方后备都跟不上，要加算力或叫 agent 查派发。`];
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
  return ["watch", f.kind, "看门狗没有这一类的说明，原始数据见下。"];
}
