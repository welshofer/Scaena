// What a build is told (PLAN 2.7): `VITE_BUNDLE`, the bundle the player and the editor open
// when the page's address names none. `just site` sets it to the site's demo deck.
interface ImportMetaEnv {
  readonly VITE_BUNDLE?: string;
}
