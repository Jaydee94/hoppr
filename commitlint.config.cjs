/**
 * Commit message rules — see https://www.conventionalcommits.org/
 *
 * Enforced on PRs by the `commitlint` job in .github/workflows/ci.yml.
 * The release workflow auto-generates release notes from these commits.
 */
module.exports = {
  extends: ['@commitlint/config-conventional'],
  rules: {
    'type-enum': [
      2,
      'always',
      [
        'feat',
        'fix',
        'perf',
        'refactor',
        'docs',
        'test',
        'chore',
        'ci',
        'build',
        'style',
        'revert',
      ],
    ],
    'subject-case': [
      2,
      'never',
      ['sentence-case', 'start-case', 'pascal-case', 'upper-case'],
    ],
    'header-max-length': [2, 'always', 100],
    'body-max-line-length': [1, 'always', 120],
  },
};
