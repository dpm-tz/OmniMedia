import clsx, { type ClassValue } from "clsx";

// Tiny re-export so feature code imports a single helper instead of clsx
// directly. Lets us swap in tailwind-merge later without rippling changes.
export const cn = (...args: ClassValue[]) => clsx(args);
