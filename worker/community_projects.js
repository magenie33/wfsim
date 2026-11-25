// SPDX-License-Identifier: AGPL-3.0-or-later
import { contributorAccounts, contributorMarks, publicContributorIdentity } from "./contributor_identity.js";

const projects = [{
  id: "simfleet", name: "simfleet",
  description: "An independent command-line client for contributing compute to WFSim.",
  links: { source: "https://github.com/superexboom/simfleet" },
  verified: true,
}];

export async function communityProjectsRoute(request, env, path) {
  if (path !== "/api/community/projects") return null;
  if (request.method !== "GET") return new Response(null, { status: 405 });
  const owners = env.ACCOUNTS ? (await env.ACCOUNTS.prepare(
    "SELECT project,account FROM relation_community_project_developers").all()).results : [];
  const people = await contributorAccounts(env, owners.map(row => row.account));
  const marks = await contributorMarks(env, people);
  return Response.json({ ok: true, projects: projects.map(project => {
    const person = people.get(owners.find(row => row.project === project.id)?.account);
    return { ...project, developer: publicContributorIdentity(person, marks[person?.id]) };
  }) }, { headers: { "cache-control": "no-store" } });
}
