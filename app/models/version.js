import Model, { attr, belongsTo, hasMany } from '@ember-data/model';

import { keepLatestTask, task } from 'ember-concurrency';
import fetch from 'fetch';
import { alias } from 'macro-decorators';
import semverParse from 'semver/functions/parse';
import { cached } from 'tracked-toolbox';

import ajax from '../utils/ajax';

const EIGHT_DAYS = 8 * 24 * 60 * 60 * 1000;

export default class Version extends Model {
  @attr num;
  @attr dl_path;
  @attr readme_path;
  @attr('date') created_at;
  @attr('date') updated_at;
  @attr downloads;
  @attr features;
  @attr yanked;
  @attr license;
  @attr crate_size;

  @belongsTo('crate', { async: false, inverse: 'versions' }) crate;

  @belongsTo('user', { async: false, inverse: null }) published_by;
  @hasMany('dependency', { async: true, inverse: 'version' }) dependencies;
  @hasMany('version-download', { async: true, inverse: null }) version_downloads;

  get crateName() {
    return this.belongsTo('crate').id();
  }

  get isNew() {
    return Date.now() - this.created_at.getTime() < EIGHT_DAYS;
  }

  @cached get isFirst() {
    let { versions } = this.crate;
    let oldestVersion = versions.sortBy('created_at')[0];
    return oldestVersion === this;
  }

  get semver() {
    return semverParse(this.num, { loose: true });
  }

  get invalidSemver() {
    return this.semver === null;
  }

  get isPrerelease() {
    if (this.invalidSemver) {
      return false;
    }

    return this.semver.prerelease.length !== 0;
  }

  get releaseTrack() {
    if (this.invalidSemver) {
      return null;
    }

    let { semver } = this;
    return semver.major >= 100 ? String(semver.major) : `${semver.major}.${semver.major === 0 ? semver.minor : 'x'}`;
  }

  @cached get isHighestOfReleaseTrack() {
    if (this.isPrerelease || this.invalidSemver) {
      return false;
    }

    let { crate, semver, releaseTrack } = this;
    let { versions } = crate;
    // find all other non-prerelease versions on the same release track
    let sameTrackVersions = versions.filter(
      it => it !== this && !it.yanked && !it.isPrerelease && !it.invalidSemver && it.releaseTrack === releaseTrack,
    );
    // check if we're the "highest"
    return sameTrackVersions.every(it => it.semver.compare(semver) === -1);
  }

  get featureList() {
    let { features } = this;
    if (typeof features !== 'object' || features === null) {
      return [];
    }

    let defaultFeatures = features.default ?? [];
    return Object.keys(features)
      .filter(name => name !== 'default')
      .sort()
      .map(name => ({ name, isDefault: defaultFeatures.includes(name), dependencies: features[name] }));
  }

  @alias('loadDepsTask.last.value.normal') normalDependencies;
  @alias('loadDepsTask.last.value.build') buildDependencies;
  @alias('loadDepsTask.last.value.dev') devDependencies;

  loadDepsTask = keepLatestTask(async () => {
    // trigger the async relationship to load the content
    let dependencies = await this.dependencies;

    let normal = dependencies.filterBy('kind', 'normal').uniqBy('crate_id');
    let build = dependencies.filterBy('kind', 'build').uniqBy('crate_id');
    let dev = dependencies.filterBy('kind', 'dev').uniqBy('crate_id');

    return { normal, build, dev };
  });

  loadReadmeTask = keepLatestTask(async () => {
    if (this.readme_path) {
      let response = await fetch(this.readme_path);
      if (!response.ok) {
        throw new Error(`README request for ${this.crateName} v${this.num} failed`);
      }

      return await response.text();
    }
  });

  loadDocsBuildsTask = task(async () => {
    return await ajax(`https://docs.rs/crate/${this.crateName}/${this.num}/builds.json`);
  });

  get hasDocsRsLink() {
    let docsBuilds = this.loadDocsBuildsTask.lastSuccessful?.value;
    return docsBuilds?.[0]?.build_status === true;
  }

  get docsRsLink() {
    if (this.hasDocsRsLink) {
      return `https://docs.rs/${this.crateName}/${this.num}`;
    }
  }

  get documentationLink() {
    let crateDocsLink = this.crate.documentation;

    // if this is *not* a docs.rs link we'll return it directly
    if (crateDocsLink && !crateDocsLink.startsWith('https://docs.rs/')) {
      return crateDocsLink;
    }

    // if we know about a successful docs.rs build, we'll return a link to that
    let { docsRsLink } = this;
    if (docsRsLink) {
      return docsRsLink;
    }

    // finally, we'll return the specified documentation link, whatever it is
    if (crateDocsLink) {
      return crateDocsLink;
    }

    return null;
  }

  yankTask = keepLatestTask(async () => {
    let response = await fetch(`/api/v1/crates/${this.crate.id}/${this.num}/yank`, { method: 'DELETE' });
    if (!response.ok) {
      throw new Error(`Yank request for ${this.crateName} v${this.num} failed`);
    }
    this.set('yanked', true);

    return await response.text();
  });

  unyankTask = keepLatestTask(async () => {
    let response = await fetch(`/api/v1/crates/${this.crate.id}/${this.num}/unyank`, { method: 'PUT' });
    if (!response.ok) {
      throw new Error(`Unyank request for ${this.crateName} v${this.num} failed`);
    }
    this.set('yanked', false);

    return await response.text();
  });
}
