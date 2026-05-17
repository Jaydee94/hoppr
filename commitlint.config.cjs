/**
 * Commit message rules — see https://www.conventionalcommits.org/
 *
 * Allowed types feed into semantic-release (see .releaserc.json).
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
