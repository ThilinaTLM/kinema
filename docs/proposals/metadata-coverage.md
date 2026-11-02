# Metadata coverage: locales, providers and facts

**Status:** Draft · **Area:** providers / UI

## Problem

Metadata is fetched in English only, from two providers, and several Detail
sections get no data even when a provider could supply it.

## Current state

- Requests use a fixed `en-US` locale. The schema stores one current value per
  field, with a single locale (see [data](../architecture/data.md#locale)).
- The providers are Cinemeta and TMDB, in an ordered priority with enrichment.
- Detail renders the cast, ratings and fact groups when data is supplied.
  Votes, certification, creators and studios are not fully wired from every
  provider.
- Episode rows have stills only when the provider supplies them.
- Search is limited to the current kind (Movies or Series).
- When Cinemeta is the primary provider, it browses only by kind and offset.
  Named catalogs (Trending, Top 250) are not offered and are never faked.

## Options

1. **Multiple locales:** store localized fields per locale, with fallback
   rules. This needs a schema design, because overwriting a value in one
   language must never make it look like the other language's snapshot still
   exists.
2. **More providers** (TVmaze, OMDb, …) behind the existing capability
   interface. Only add them if their terms suit a desktop app where users
   bring their own key.
3. **Search across both kinds,** with grouped Movies and Series results, instead
   of the current kind-scoped search.
4. **Named catalogs** where the backend supports them, such as Cinemeta
   catalogs or TMDB lists.
5. **Fill in the missing facts** for each provider (votes, certification,
   creators, studios), and add "Show all N" for long lists.

## Open questions

- Which locale should the app pick by default: the OS language, or a setting?
- Which additional provider is worth its maintenance cost?
