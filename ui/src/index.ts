// Config validation must run first — before any other imports.
import { config } from './config';

export { config };
export type { Config } from './config';
export { default as PrecisionFintech } from '../components/PrecisionFintech';
export type {
	PrecisionFintechProps,
	PrecisionFintechMetric,
	PrecisionFintechPosition,
} from '../components/PrecisionFintech';
