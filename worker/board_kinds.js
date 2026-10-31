// SPDX-License-Identifier: AGPL-3.0-or-later
// THE BOARD'S PRODUCERS, by name — docs/BOARD.md §"The fact factory". A module
// that imports nothing, because the live board's scripts on the board server read
// it too (`order_credit.mjs`) and that server carries only what they import.
export const BOARD_KINDS = ["new_build", "survey_build", "refresh", "sweep"];
export const BOARD_WORK_KINDS = [...BOARD_KINDS, "board_review"];
