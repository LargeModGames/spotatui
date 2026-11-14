import { EN, type GuiStrings } from "./i18n";
import { useState } from "react";
import type { OnboardingQuestion } from "./bindings/OnboardingQuestion";
import type { OnboardingReply } from "./bindings/OnboardingReply";
import type { OnboardingView } from "./bindings/OnboardingView";
import type { Source } from "./bindings/Source";

/** The first-launch questions, answered in the page; a placeholder layout like the rest. */
export function Onboarding({
  s = EN,
  view,
  connected,
  onReply,
}: {
  s?: GuiStrings;
  view: OnboardingView;
  connected: boolean;
  onReply: (reply: OnboardingReply) => void;
}) {
  return (
    <section className="onboarding">
      <pre className="transcript">{view.transcript}</pre>
      {view.pending && (
        <Question
          s={s}
          key={view.pending.seq}
          question={view.pending}
          connected={connected}
          onReply={onReply}
        />
      )}
    </section>
  );
}

function Question({
  s = EN,
  question: { seq, ask },
  connected,
  onReply,
}: {
  s?: GuiStrings;
  question: OnboardingQuestion;
  connected: boolean;
  onReply: (reply: OnboardingReply) => void;
}) {
  const [text, setText] = useState("");
  const [picked, setPicked] = useState<Source[]>([]);
  switch (ask.kind) {
    case "Line":
      return (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onReply({ seq, answer: { kind: "Line", text } });
          }}
        >
          <label>
            {ask.prompt}
            <input
              type={ask.masked ? "password" : "text"}
              value={text}
              onChange={(event) => setText(event.target.value)}
              autoFocus
            />
          </label>
          <button type="submit" disabled={!connected}>
            {s.continue_button}
          </button>
        </form>
      );
    case "Confirm":
      return (
        <div className="onboarding-question">
          <h2>{ask.title}</h2>
          <p>{ask.body}</p>
          <p>{ask.question}</p>
          <div className="onboarding-actions">
            <button
              className="onboarding-primary"
              disabled={!connected}
              onClick={() =>
                onReply({ seq, answer: { kind: "Confirm", yes: true } })
              }
            >
              {s.yes}
            </button>
            <button
              disabled={!connected}
              onClick={() =>
                onReply({ seq, answer: { kind: "Confirm", yes: false } })
              }
            >
              {s.no}
            </button>
          </div>
        </div>
      );
    case "PickSources":
      return (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onReply({ seq, answer: { kind: "Sources", picked } });
          }}
        >
          <h2>{s.choose_sources}</h2>
          <p>{s.choose_sources_hint}</p>
          {ask.options.map(({ source, label, note }) => (
            <label key={source}>
              <input
                type="checkbox"
                checked={picked.includes(source)}
                onChange={(event) =>
                  setPicked((current) =>
                    event.target.checked
                      ? [...current, source]
                      : current.filter((other) => other !== source),
                  )
                }
              />
              {label} ({note})
            </label>
          ))}
          <button type="submit" disabled={!connected}>
            {s.continue_button}
          </button>
        </form>
      );
  }
}
