import { useState } from "react";
import type { OnboardingQuestion } from "./bindings/OnboardingQuestion";
import type { OnboardingReply } from "./bindings/OnboardingReply";
import type { OnboardingView } from "./bindings/OnboardingView";
import type { Source } from "./bindings/Source";

/** The first-launch questions, answered in the page; a placeholder layout like the rest. */
export function Onboarding({
  view,
  onReply,
}: {
  view: OnboardingView;
  onReply: (reply: OnboardingReply) => void;
}) {
  return (
    <section className="onboarding">
      <pre className="transcript">{view.transcript}</pre>
      {view.pending && (
        <Question
          key={view.pending.seq}
          question={view.pending}
          onReply={onReply}
        />
      )}
    </section>
  );
}

function Question({
  question: { seq, ask },
  onReply,
}: {
  question: OnboardingQuestion;
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
              type={ask.secret ? "password" : "text"}
              value={text}
              onChange={(event) => setText(event.target.value)}
              autoFocus
            />
          </label>
          <button type="submit">Continue</button>
        </form>
      );
    case "Confirm":
      return (
        <div>
          <h2>{ask.title}</h2>
          <pre>{ask.body}</pre>
          <p>{ask.question}</p>
          <button
            onClick={() =>
              onReply({ seq, answer: { kind: "Confirm", yes: true } })
            }
          >
            Yes
          </button>
          <button
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
          {ask.options.map((option) => (
            <label key={option}>
              <input
                type="checkbox"
                checked={picked.includes(option)}
                onChange={(event) =>
                  setPicked((current) =>
                    event.target.checked
                      ? [...current, option]
                      : current.filter((source) => source !== option),
                  )
                }
              />
              {option}
            </label>
          ))}
          <button type="submit">Continue</button>
        </form>
      );
  }
}
