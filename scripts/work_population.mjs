// Linear gas prediction needs population counter sums and a bounded native sample.
export const COUNTS = ['runs', 'shots', 'pellets', 'procs', 'dot_ticks', 'field_ticks', 'kills', 'hits', 'body_shots'];
export const countsOf = q => COUNTS.map(name => Number(q.counts?.[name] || 0));
export const population = () => ({ questions: 0, priced: 0, accounts: new Map(), sample: [] });

export function addQuestion(all, q, limit, random = Math.random) {
  all.questions++;
  if (q.counts && q.work > 0) {
    all.priced++;
    const counts = countsOf(q);
    for (const account of q.accounts) {
      const row = all.accounts.get(account) || { account, old: 0, answers: 0, counts: COUNTS.map(() => 0) };
      row.old += q.work; row.answers++;
      counts.forEach((value, i) => { row.counts[i] += value; });
      all.accounts.set(account, row);
    }
  }
  const index = all.questions <= limit ? all.questions - 1 : Math.floor(random() * all.questions);
  if (index < limit) all.sample[index] = q;
}

export function pricedPopulation(all, weights) {
  const dot = counts => counts.reduce((sum, value, i) => sum + value * weights[i], 0);
  const accounts = new Map(all.accounts.map(row => [row.account,
    { account: row.account, old: row.old, answers: row.answers, gas: dot(row.counts), runs: row.counts[0] }]));
  let priced = all.priced;
  for (const q of all.sample.filter(q => q.exact && q.work > 0)) {
    if (!q.counts) priced++;
    for (const account of q.accounts) {
      const row = accounts.get(account) || { account, old: 0, answers: 0, gas: 0, runs: 0 };
      if (!q.counts) { row.old += q.work; row.answers++; }
      row.gas += q.exact.gas - (q.counts ? dot(countsOf(q)) : 0);
      row.runs += q.exact.runs - Number(q.counts?.runs || 0);
      accounts.set(account, row);
    }
  }
  return { priced, accounts };
}

export const populationDocument = (all, meta) => ({ ...meta, v: 2, questions: all.questions,
  priced: all.priced, accounts: [...all.accounts.values()], sample: all.sample.map(q => ({
    id: q.id, work: q.work, counts: q.counts, accounts: q.accounts, exact: q.exact || null,
  })) });
