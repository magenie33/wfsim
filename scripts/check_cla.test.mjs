// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from "node:assert/strict";
import { test } from "node:test";
import { recordedAccounts, verifyPull } from "./check_cla.mjs";

const repo = "magenie33/wfsim";
const head = "a".repeat(40);
const firstCommit = "b".repeat(40);
const revision = "c".repeat(40);
const owner = { id: "owner-node", databaseId: 1, login: "maintainer", __typename: "User" };
const visitor = { id: "visitor-node", databaseId: 2, login: "visitor", __typename: "User" };
const coauthor = { id: "coauthor-node", databaseId: 3, login: "coauthor", __typename: "User" };
const bot = { id: "bot-node", login: "dependabot", __typename: "Bot" };
const signed = id => `${id} ${revision} https://github.com/${repo}/pull/9#issuecomment-12`;

function commit(oid, users, hasNextPage = false) {
  return { commit: { oid, authors: { pageInfo: { hasNextPage }, nodes: users.map(user => ({ user })) } } };
}

function page(author, nodes, options = {}) {
  const { headRefOid = head, headRefName = "topic", totalCount = nodes.length, hasNextPage = false } = options;
  return { data: { repository: { owner, pullRequest: {
    baseRefName: "main", headRefOid, headRefName, author,
    commits: { nodes, totalCount, pageInfo: { hasNextPage } },
  } } } };
}

test("accepts only complete records linked to this repository", () => {
  assert.deepEqual([...recordedAccounts("# record\n" + signed(2) + "\r\n", repo)], ["2"]);
  assert.deepEqual([...recordedAccounts(`3 ${revision} https://github.com/${repo}/issues/12`, repo)], ["3"]);
  for (const bad of ["2", signed(2).replace(repo, "someone/else"),
    signed(2).replace("#issuecomment-12", ""), signed(2).replace("https:", "http:")]) {
    assert.throws(() => recordedAccounts(bad, repo), /CLA record/);
  }
  assert.throws(() => recordedAccounts(signed(2) + "\n" + signed(2), repo), /Duplicate/);
});

test("first party and documented outside authors pass", () => {
  assert.equal(verifyPull([page(owner, [commit(head, [owner])])], new Set()), head);
  const accounts = recordedAccounts(signed(2) + "\n" + signed(3), repo);
  const pull = page(visitor, [commit(head, [visitor, coauthor])]);
  assert.equal(verifyPull([pull], accounts, head), head);
  const renamed = page({ ...visitor, login: "renamed-account" }, [commit(head, [visitor, coauthor])]);
  assert.equal(verifyPull([renamed], accounts), head);
});

test("signed opener cannot authorize an unsigned or unlinked coauthor", () => {
  const accounts = recordedAccounts(signed(2), repo);
  assert.throws(() => verifyPull([page(visitor, [commit(head, [visitor, coauthor])])], accounts), /@coauthor/);
  assert.throws(() => verifyPull([page(visitor, [commit(head, [visitor, null])])], accounts), /not linked/);
  assert.throws(() => verifyPull([page(visitor, [commit(head, [owner])])], new Set()), /@visitor/);
  assert.throws(() => verifyPull([page(owner, [commit(head, [visitor])])], new Set()), /@visitor/);
});

test("validates every page at one head and fails closed on missing authors", () => {
  const first = page(owner, [commit(firstCommit, [owner])], { totalCount: 2, hasNextPage: true });
  const last = page(owner, [commit(head, [owner])], { totalCount: 2 });
  assert.equal(verifyPull([first, last], new Set()), head);
  assert.throws(() => verifyPull([first], new Set()), /all commits/);
  assert.throws(() => verifyPull([first, last], new Set(), firstCommit), /head changed/);
  assert.throws(() => verifyPull([page(owner, [commit(head, [owner], true)])], new Set()), /Incomplete authorship/);
});

test("authentic Dependabot commits pass with their own author only", () => {
  const options = { headRefName: "dependabot/cargo/package" };
  const botAuthor = { ...bot, login: "dependabot[bot]" };
  assert.equal(verifyPull([page(bot, [commit(head, [botAuthor])], options)], new Set()), head);
  assert.throws(() => verifyPull([page(bot, [commit(head, [botAuthor, visitor])], options)], new Set()), /@visitor/);
  assert.throws(() => verifyPull([page(visitor, [commit(head, [botAuthor])], options)], new Set()), /Missing/);
});
