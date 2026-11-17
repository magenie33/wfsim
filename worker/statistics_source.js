// SPDX-License-Identifier: AGPL-3.0-or-later
// Source attribution is shared by the projection and the demand formatter.
export const statisticsSourceSql=alias=>`CASE WHEN ${alias}.initiator='system' THEN 'system' WHEN ${alias}.initiator='anonymous' OR ${alias}.initiator LIKE 'user:%' OR ${alias}.initiator LIKE 'bot:%' THEN 'requested' ELSE 'unattributed' END`;
