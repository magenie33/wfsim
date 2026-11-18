// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const SHA = /^[a-f0-9]{40}$/;
const ACCOUNT = /^[1-9][0-9]*$/;
const QUERY = `query($owner:String!, $repo:String!, $number:Int!, $endCursor:String) {
  repository(owner:$owner, name:$repo) {
    owner { ... on User { databaseId } }
    pullRequest(number:$number) {
      baseRefName headRefName headRefOid
      author { __typename login ... on Node { id } ... on User { databaseId } }
      commits(first:50, after:$endCursor) {
        totalCount pageInfo { hasNextPage endCursor }
        nodes { commit { oid authors(first:100) {
          pageInfo { hasNextPage }
          nodes { user { databaseId id login } }
        } } }
      }
    }
  }
}`;

export function recordedAccounts(source, repository) {
  const accounts = new Set();
  for (const line of source.split(/\r?\n/).map(line => line.trim())) {
    if (!line || line.startsWith("#")) continue;
    const [id, revision, link, extra] = line.split(/\s+/);
    if (!ACCOUNT.test(id) || !SHA.test(revision) || extra || !link) {
      throw new Error("Invalid CLA record. Use account ID, agreement commit and signature permalink.");
    }
    const url = new URL(link);
    const prefix = '/' + repository + '/';
    const path = url.pathname.slice(prefix.length);
    const issue = /^issues\/[1-9][0-9]*$/.test(path);
    const pull = /^pull\/[1-9][0-9]*$/.test(path);
    const comment = /^#issuecomment-[1-9][0-9]*$/.test(url.hash);
    if (url.origin !== "https://github.com" || url.username || url.password || url.search ||
        !url.pathname.startsWith(prefix) || !(comment && (issue || pull) || issue && !url.hash)) {
      throw new Error("A CLA record must link to its signature on this repository.");
    }
    if (accounts.has(id)) throw new Error(`Duplicate CLA record for GitHub account ${id}`);
    accounts.add(id);
  }
  return accounts;
}

export function verifyPull(pages, accounts, expectedHead = "") {
  const original = pages[0]?.data?.repository;
  const pull = original?.pullRequest;
  if (!pull || !SHA.test(pull.headRefOid) || pull.baseRefName !== "main") {
    throw new Error("Cannot verify a pull request targeting main.");
  }
  if (expectedHead && pull.headRefOid !== expectedHead) {
    throw new Error("The pull request head changed during the CLA check.");
  }
  const commits = new Set();
  const authors = new Map();
  for (const page of pages) {
    const snapshot = page.data?.repository?.pullRequest;
    if (page.errors?.length || snapshot?.headRefOid !== pull.headRefOid ||
        snapshot.baseRefName !== pull.baseRefName || snapshot.author?.id !== pull.author?.id ||
        snapshot.commits.totalCount !== pull.commits.totalCount) {
      throw new Error("Could not read a single complete pull request snapshot.");
    }
    for (const {commit} of snapshot.commits.nodes) {
      if (commits.has(commit.oid) || commit.authors.pageInfo.hasNextPage || !commit.authors.nodes.length) {
        throw new Error(`Incomplete authorship for commit ${commit.oid}`);
      }
      commits.add(commit.oid);
      for (const {user} of commit.authors.nodes) {
        if (!user?.id) throw new Error(`An author of ${commit.oid} is not linked to a GitHub account.`);
        authors.set(user.id, user);
      }
    }
  }
  if (!commits.has(pull.headRefOid) || commits.size !== pull.commits.totalCount ||
      pages.at(-1).data.repository.pullRequest.commits.pageInfo.hasNextPage) {
    throw new Error("Could not verify all commits of the pull request.");
  }
  if (!pull.author?.id) throw new Error("The pull request author has no verifiable account.");
  if (pull.author.__typename === "Bot" && pull.author.login === "dependabot" &&
      pull.headRefName.startsWith("dependabot/") && authors.size === 1 && authors.has(pull.author.id)) {
    return pull.headRefOid;
  }
  authors.set(pull.author.id, pull.author);
  const owner = original.owner.databaseId;
  const missing = [...authors.values()].filter(author =>
    !author.databaseId || author.databaseId !== owner && !accounts.has(String(author.databaseId)));
  if (missing.length) {
    throw new Error(`Missing verified CLA for ${missing.map(author => `@${author.login}`).join(", ")}. See CLA.md §7.`);
  }
  return pull.headRefOid;
}

function main() {
  const [number, expectedHead = ""] = process.argv.slice(2);
  const repository = process.env.GITHUB_REPOSITORY || "magenie33/wfsim";
  if (!ACCOUNT.test(number) || expectedHead && !SHA.test(expectedHead)) {
    throw new Error("Usage: node scripts/check_cla.mjs <pull request number> [expected head SHA]");
  }
  const [owner, repo] = repository.split("/");
  const accounts = recordedAccounts(readFileSync(new URL("../.github/cla-signed.txt", import.meta.url), "utf8"), repository);
  const pages = JSON.parse(execFileSync("gh", [
    "api", "graphql", "--paginate", "--slurp", "-f", `query=${QUERY}`,
    "-f", `owner=${owner}`, "-f", `repo=${repo}`, "-F", `number=${number}`,
  ], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024, timeout: 60_000 }));
  const head = pages[0]?.data?.repository?.pullRequest?.headRefOid;
  if (process.env.GITHUB_OUTPUT && SHA.test(head) && (!expectedHead || head === expectedHead)) {
    appendFileSync(process.env.GITHUB_OUTPUT, `head_sha=${head}\n`);
  }
  console.log(`Contribution rights recorded for ${verifyPull(pages, accounts, expectedHead)}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
