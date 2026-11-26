import { resolve } from 'node:path';

const cargoTarget = resolve(import.meta.dirname, '../..', process.env.CARGO_TARGET_DIR ?? 'target');

export const SERVER = `${cargoTarget}/debug/explore-page-server`;
