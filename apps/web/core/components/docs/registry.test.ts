import { describe, expect, it } from "vitest";
import { DEFAULT_DOC_TOPIC_SLUG, DOC_TOPIC_GROUPS, DOC_TOPICS, getTopicBySlug, getTopicNeighbors } from "./registry";

describe("docs registry", () => {
  it("has unique slugs", () => {
    const slugs = DOC_TOPICS.map((topic) => topic.slug);
    expect(new Set(slugs).size).toBe(slugs.length);
  });

  it("assigns every topic to a known group", () => {
    const groupKeys = new Set(DOC_TOPIC_GROUPS.map((group) => group.key));
    DOC_TOPICS.forEach((topic) => expect(groupKeys.has(topic.group)).toBe(true));
  });

  it("keeps the default topic first in order", () => {
    expect(DOC_TOPICS[0]?.slug).toBe(DEFAULT_DOC_TOPIC_SLUG);
  });

  it("resolves known slugs and rejects unknown ones", () => {
    expect(getTopicBySlug("tickets")?.slug).toBe("tickets");
    expect(getTopicBySlug("not-a-topic")).toBeUndefined();
  });

  it("registers the metric sources integration topic", () => {
    const topic = getTopicBySlug("metric-sources");
    expect(topic?.group).toBe("settings");
    expect(topic?.title).toContain("Metric sources");
  });

  it("returns neighbors with correct edges", () => {
    const first = getTopicNeighbors(DOC_TOPICS[0].slug);
    expect(first.prev).toBeUndefined();
    expect(first.next?.slug).toBe(DOC_TOPICS[1].slug);

    const lastIndex = DOC_TOPICS.length - 1;
    const last = getTopicNeighbors(DOC_TOPICS[lastIndex].slug);
    expect(last.next).toBeUndefined();
    expect(last.prev?.slug).toBe(DOC_TOPICS[lastIndex - 1].slug);

    const middle = getTopicNeighbors(DOC_TOPICS[3].slug);
    expect(middle.prev?.slug).toBe(DOC_TOPICS[2].slug);
    expect(middle.next?.slug).toBe(DOC_TOPICS[4].slug);
  });

  it("returns empty neighbors for unknown slugs", () => {
    expect(getTopicNeighbors("not-a-topic")).toEqual({});
  });
});
