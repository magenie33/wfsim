// SPDX-License-Identifier: AGPL-3.0-or-later
// A NAME SHOWN TO OTHERS passes this first: a username, a display name, a name
// thanked in a chat. Shown on public pages and relayed into QQ groups, a name
// is something WFSim publishes, so a name carrying a word from the bundled list
// (vendor/lexicon, MIT) is refused. Matched after folding case, width and every
// mark that is not a letter or a digit, so spacing a word out does not pass it.
import { WORDS } from "./vendor/lexicon/words.js";

export const foldName = (s) => String(s || "").normalize("NFKC").toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");

// A NAME SAYS WHO SOMEONE IS AND POINTS NOWHERE ELSE: any handle passes, a
// Bilibili one included, but a link, a way to reach someone, an offer to trade
// or a claim to speak for WFSim turns a ranking into an advert. Digits alone
// never count — `dna980560` is a handle; a number is contact only beside a word
// that says so. A domain is matched before folding, which would drop its dots.
const TLD = "com|cn|net|org|top|xyz|io|app|me|cc|tv|vip|shop|store|site|online|club|info|link|fun|icu|ltd|work|co|gg";
const LINK = new RegExp(`https?:|www\\.|[\\p{L}\\p{N}-]\\s*[.。．]\\s*(?:${TLD})(?![a-z])`, "u");
const LINK_SPELLED = new RegExp(`(?:点|dot)(?:${TLD})`);
const CONTACT = /(?:qq|扣扣|企鹅|微信|weixin|威信|薇信|vx|wx|群号|加群|q群|电话|手机|tel)\d{5,}|\d{5,}(?:qq|群)|加微|加v|加q|1[3-9]\d{9}/;
const ELSEWHERE = ["代刷", "代练", "代打", "代肝", "出售", "收购", "接单", "出白金", "收白金", "卖白金", "白金出",
  "淘宝", "闲鱼", "拼多多", "店铺", "官方", "管理员", "客服", "wfsim作者", "wfsimofficial", "wfsim团队"];

const pointsElsewhere = (s, n) => LINK.test(String(s).normalize("NFKC").toLowerCase())
  || LINK_SPELLED.test(n) || CONTACT.test(n) || ELSEWHERE.some((w) => n.includes(w));

export function nameBlocked(s) {
  const n = foldName(s);
  return !!n && (WORDS.some((w) => n.includes(w)) || pointsElsewhere(s, n));
}

/// THE NAME OTHERS ARE SHOWN: the display name, or the username where there is
/// none or where it would not pass today — a name set before a rule existed is
/// hidden by it, never deleted.
export const shownName = (display, username) => (display && !nameBlocked(display) ? display : username);
