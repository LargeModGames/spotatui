import { useState } from "react";
import type { OnboardingQuestion } from "./bindings/OnboardingQuestion";
import type { OnboardingReply } from "./bindings/OnboardingReply";
import type { OnboardingView } from "./bindings/OnboardingView";
import type { Source } from "./bindings/Source";

/** The first-launch questions, answered in the page; a placeholder layout like the rest. */
export function Onboarding({
  view,
  connected,
  onReply,
}: {
  view: OnboardingView;
  connected: boolean;
  onReply: (reply: OnboardingReply) => void;
}) {
  return (
    <section className="onboarding">
      <pre className="transcript">{view.transcript}</pre>
      {view.pending && (
        <Question
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
  question: { seq, ask },
  connected,
  onReply,
}: {
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
            Continue
          </button>
        </form>
      );
    case "Confirm":
      return (
        <div>
          <h2>{ask.title}</h2>
          <pre>{ask.body}</pre>
          <p>{ask.question}</p>
          <button
            disabled={!connected}
            onClick={() =>
              onReply({ seq, answer: { kind: "Confirm", yes: true } })
            }
          >
            Yes
          </button>
          <button
            disabled={!connected}
            onClick={() =>
              onReply({ seq, answer: { kind: "Confirm", yes: false } })
            }
          >
            No
          </button>
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
          <h2>Choose your music sources</h2>
          <p>
            Leave all unchecked to set up Spotify only; you can add or switch
            sources later.
          </p>
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
            Continue
          </button>
        </form>
      );
  }
}
