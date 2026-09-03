# Changelog

## [0.10.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.4...fun-refactor-v0.10.0) (2026-09-03)


### Features

* **graph:** trace lexical callable values ([#235](https://github.com/e6qu/fun-refactor/issues/235)) ([3a6c1ec](https://github.com/e6qu/fun-refactor/commit/3a6c1ec23ea74632307d00fcc78fcaddd4cf5fb5))
* **inline:** substitute around literal data ([#230](https://github.com/e6qu/fun-refactor/issues/230)) ([472752d](https://github.com/e6qu/fun-refactor/commit/472752db9be05014b4cc1a8aadd91e24d4c3141c))
* **inline:** substitute resolved parameter references ([#231](https://github.com/e6qu/fun-refactor/issues/231)) ([a1b3602](https://github.com/e6qu/fun-refactor/commit/a1b3602e7f1d756ceffb95705c24d5980157aa41))


### Fixes

* **inline:** refuse parameter field-name collisions ([#227](https://github.com/e6qu/fun-refactor/issues/227)) ([caae510](https://github.com/e6qu/fun-refactor/commit/caae510099a1b72fdd07bf24dc866f9d44cf17ca))
* **inline:** refuse Rust struct shorthand ([#224](https://github.com/e6qu/fun-refactor/issues/224)) ([d049d8e](https://github.com/e6qu/fun-refactor/commit/d049d8e0e38553c573012ce2a482bbc830841ca2))
* **inline:** refuse substitutions inside character literals ([#229](https://github.com/e6qu/fun-refactor/issues/229)) ([5cfafab](https://github.com/e6qu/fun-refactor/commit/5cfafab64d42b9661b9882fc0bead52ed6a05e62))
* **inline:** refuse substitutions inside literals ([#228](https://github.com/e6qu/fun-refactor/issues/228)) ([47c53c4](https://github.com/e6qu/fun-refactor/commit/47c53c4edf55478ad54419f59d1bcc15d754a006))
* **inline:** refuse TypeScript object shorthand ([#225](https://github.com/e6qu/fun-refactor/issues/225)) ([ba24020](https://github.com/e6qu/fun-refactor/commit/ba24020679dc3f79567e053cc720f2e09aa9d406))
* **inline:** refuse TypeScript object shorthand calls ([#226](https://github.com/e6qu/fun-refactor/issues/226)) ([80f9416](https://github.com/e6qu/fun-refactor/commit/80f9416edc45945bd949bd6611853e3bd5f6f3b2))
* **lean:** resolve chained branch bindings ([#234](https://github.com/e6qu/fun-refactor/issues/234)) ([5a98387](https://github.com/e6qu/fun-refactor/commit/5a983871956ea999224e6c682b7e679558f55c26))
* **signature:** refuse changes through expansions ([#222](https://github.com/e6qu/fun-refactor/issues/222)) ([d565ec9](https://github.com/e6qu/fun-refactor/commit/d565ec923fe9db7f0f796482aceebbfb991c66a5))

## [0.9.4](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.3...fun-refactor-v0.9.4) (2026-09-01)


### Fixes

* **signature:** refuse positional changes after keywords ([#220](https://github.com/e6qu/fun-refactor/issues/220)) ([ade1077](https://github.com/e6qu/fun-refactor/commit/ade1077b6f579aaa652205904c3ecd88b58e1861))

## [0.9.3](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.2...fun-refactor-v0.9.3) (2026-09-01)


### Fixes

* **inline:** refuse shadowed closure parameters ([#218](https://github.com/e6qu/fun-refactor/issues/218)) ([4fdf63a](https://github.com/e6qu/fun-refactor/commit/4fdf63a69530a414ade40f1449fa057582bf2ee4))
* **inline:** refuse unsupported substitutions ([#219](https://github.com/e6qu/fun-refactor/issues/219)) ([6076f6a](https://github.com/e6qu/fun-refactor/commit/6076f6a52e8a5dcc7633d7072cd7c42c2d5c6709))


### Tests

* **imports:** dogfood self cleanup plan ([#213](https://github.com/e6qu/fun-refactor/issues/213)) ([db220f6](https://github.com/e6qu/fun-refactor/commit/db220f67fefd9901d4bd90790875b80f40d8feba))
* **inline:** cover mutable Rust receivers ([#216](https://github.com/e6qu/fun-refactor/issues/216)) ([d9456c6](https://github.com/e6qu/fun-refactor/commit/d9456c67cad5b83bbd27d1ada87b369e4bafb0b9))
* **inline:** cover Rust method receivers ([#215](https://github.com/e6qu/fun-refactor/issues/215)) ([09fff5f](https://github.com/e6qu/fun-refactor/commit/09fff5f9b21a92958086b7106529c44daf9c00ea))
* **inline:** cover typed Rust receivers ([#217](https://github.com/e6qu/fun-refactor/issues/217)) ([dd0dfcb](https://github.com/e6qu/fun-refactor/commit/dd0dfcbe819328d8aa0a48741d12638f8e113358))

## [0.9.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.1...fun-refactor-v0.9.2) (2026-09-01)


### Fixes

* **inline:** substitute Rust method receivers ([#212](https://github.com/e6qu/fun-refactor/issues/212)) ([1abc39a](https://github.com/e6qu/fun-refactor/commit/1abc39a1fb6cbb6cfa0d13f6644a3a6b747c7f39))


### Tests

* **kernels:** audit self extraction edits ([#209](https://github.com/e6qu/fun-refactor/issues/209)) ([741163a](https://github.com/e6qu/fun-refactor/commit/741163a59bcb09ff325dccb1c198d23af0e9ae8e))
* **kernels:** audit self inline edits ([#211](https://github.com/e6qu/fun-refactor/issues/211)) ([bc7c325](https://github.com/e6qu/fun-refactor/commit/bc7c325f75e966e896eb1b7a2c1dbc7f499a3fb4))

## [0.9.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.0...fun-refactor-v0.9.1) (2026-09-01)


### Tests

* **kernels:** audit self move edits ([#207](https://github.com/e6qu/fun-refactor/issues/207)) ([72892c4](https://github.com/e6qu/fun-refactor/commit/72892c4a85b0d9c5be689a4403d3a83c536965ef))

## [0.9.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.3...fun-refactor-v0.9.0) (2026-09-01)


### Features

* **kernels:** check byte-native positions ([#205](https://github.com/e6qu/fun-refactor/issues/205)) ([67be33e](https://github.com/e6qu/fun-refactor/commit/67be33edaee8e577996d7af54482f2cbbc21e857))


### Tests

* **kernels:** audit self signature edits ([#204](https://github.com/e6qu/fun-refactor/issues/204)) ([b3c9105](https://github.com/e6qu/fun-refactor/commit/b3c910504ded67be3f765205208ffff2f6066c56))
* **kernels:** dogfood edit plans on fr ([#202](https://github.com/e6qu/fun-refactor/issues/202)) ([39ff80f](https://github.com/e6qu/fun-refactor/commit/39ff80fa42e7f4185769b07b04459dbb4cefb021))

## [0.8.3](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.2...fun-refactor-v0.8.3) (2026-08-31)


### Fixes

* keep Python reassignments live ([#199](https://github.com/e6qu/fun-refactor/issues/199)) ([e169e91](https://github.com/e6qu/fun-refactor/commit/e169e9181a733c71d920f624d8896902e70485c0))

## [0.8.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.1...fun-refactor-v0.8.2) (2026-08-31)


### Tests

* name the column where a `do`-level `else` stops belonging to its own `if` ([#197](https://github.com/e6qu/fun-refactor/issues/197)) ([95f7ba9](https://github.com/e6qu/fun-refactor/commit/95f7ba9d6888794d71afd2a626d699ed754a6e9b))

## [0.8.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.0...fun-refactor-v0.8.1) (2026-08-31)


### Build

* a parse table has no readable diff, and saying so lets a review load ([#195](https://github.com/e6qu/fun-refactor/issues/195)) ([2803d35](https://github.com/e6qu/fun-refactor/commit/2803d353c7a53b91e438420d4dc97f7554d7a7f6))

## [0.8.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.7.0...fun-refactor-v0.8.0) (2026-08-31)


### Features

* Lean is a source as well as a target, and the two lists meet again ([#192](https://github.com/e6qu/fun-refactor/issues/192)) ([dfb4a48](https://github.com/e6qu/fun-refactor/commit/dfb4a48625a86878a63c8781dce9f542dea62bda))

## [0.7.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.6.0...fun-refactor-v0.7.0) (2026-08-31)


### Features

* Lean is a translate target, and the reader and writer lists part ([#191](https://github.com/e6qu/fun-refactor/issues/191)) ([623abea](https://github.com/e6qu/fun-refactor/commit/623abeacf3c0e9583fba1ec49d341d6099da819f))


### Documentation

* a plan for specs in Lean, and four headings that had stopped being true ([#189](https://github.com/e6qu/fun-refactor/issues/189)) ([7d4f6f4](https://github.com/e6qu/fun-refactor/commit/7d4f6f419b2f4170b9dcf17f619fec91a013823d))

## [0.6.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.5.2...fun-refactor-v0.6.0) (2026-08-30)


### Features

* Lean 4, the nineteenth language, read but not yet written ([#187](https://github.com/e6qu/fun-refactor/issues/187)) ([5c61ab9](https://github.com/e6qu/fun-refactor/commit/5c61ab9cc738bac2dabdda76a70dd9393cc385ac))

## [0.5.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.5.1...fun-refactor-v0.5.2) (2026-08-30)


### Fixes

* a group that lost its own symbol, and the sample that hid it ([#185](https://github.com/e6qu/fun-refactor/issues/185)) ([fc393a4](https://github.com/e6qu/fun-refactor/commit/fc393a4f0dabbe09b6d6f8ad6b9a07a8922ba94c))

## [0.5.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.5.0...fun-refactor-v0.5.1) (2026-08-30)


### Refactoring

* the passive goes, and so does the rule that missed most of it ([#183](https://github.com/e6qu/fun-refactor/issues/183)) ([08bb3db](https://github.com/e6qu/fun-refactor/commit/08bb3db9a2906fe2ac74a30554c4412974fe4bb5))

## [0.5.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.4.0...fun-refactor-v0.5.0) (2026-08-30)


### Features

* every target extracts a region that returns, and no comment speaks passively ([#181](https://github.com/e6qu/fun-refactor/issues/181)) ([027ec5f](https://github.com/e6qu/fun-refactor/commit/027ec5f6479942e6920de1c7804db142878812f6))

## [0.4.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.3.0...fun-refactor-v0.4.0) (2026-08-28)


### Features

* extract a region that returns, in the targets that can say it ([#178](https://github.com/e6qu/fun-refactor/issues/178)) ([565540e](https://github.com/e6qu/fun-refactor/commit/565540e69ea63a6254517191389baf18ba1feedd))

## [0.3.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.2.2...fun-refactor-v0.3.0) (2026-08-28)


### Features

* fr tells an agent what a recipe may say, and four defects it found ([#175](https://github.com/e6qu/fun-refactor/issues/175)) ([b51038d](https://github.com/e6qu/fun-refactor/commit/b51038d5d68cb2be82023eb863b6d46c14cfb078))

## [0.2.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.2.1...fun-refactor-v0.2.2) (2026-08-28)


### Fixes

* four defects fr found in fr ([#173](https://github.com/e6qu/fun-refactor/issues/173)) ([7ad30a1](https://github.com/e6qu/fun-refactor/commit/7ad30a1636d5e395ce908696c7ddd94fa6b3c386))

## [0.2.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.2.0...fun-refactor-v0.2.1) (2026-08-28)


### Fixes

* name an archive from the version, and leave the tag alone ([#172](https://github.com/e6qu/fun-refactor/issues/172)) ([e7e2a13](https://github.com/e6qu/fun-refactor/commit/e7e2a1326e777d284913ef910c61d0d44158fc54))
* the release ships all five artifacts, and names them properly ([#170](https://github.com/e6qu/fun-refactor/issues/170)) ([7045923](https://github.com/e6qu/fun-refactor/commit/704592396eb7b15b51d763818470a31ad9a2de3b))

## [0.2.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.1.0...fun-refactor-v0.2.0) (2026-08-27)


### Features

* tagged releases with built binaries, and 11,413 fewer lines of comment ([#167](https://github.com/e6qu/fun-refactor/issues/167)) ([e7cd2a5](https://github.com/e6qu/fun-refactor/commit/e7cd2a5a9d2bb035f666cd3dac4f604e0460583e))


### Fixes

* the first release threw before it built anything ([#168](https://github.com/e6qu/fun-refactor/issues/168)) ([b0401d0](https://github.com/e6qu/fun-refactor/commit/b0401d0c7ede66094dd641b818d7c822a4a0d524))
