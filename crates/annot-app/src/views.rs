//! The user interface.

use std::rc::Rc;
use std::time::Duration;

use annot_engine::{Confidence, Role};
use annot_model::Element;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::browser;
use crate::session::{Counts, Session, Status};

type SessionSignal = RwSignal<Option<Session>, LocalStorage>;

/// The example model offered on the start screen.
const EXAMPLE: &str = include_str!("../assets/glycolysis-example.ant");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Molecules,
    Reactions,
    Compartments,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Review,
    Done,
    Nothing,
}

impl Filter {
    fn keeps(self, s: &Status) -> bool {
        match self {
            Filter::All => true,
            Filter::Review => matches!(s, Status::Suggested { .. }),
            Filter::Done => matches!(s, Status::Identified { .. } | Status::AnnotatedElsewhere { .. }),
            Filter::Nothing => matches!(s, Status::Unknown),
        }
    }
}

/// Shared by every component.
#[derive(Clone, Copy)]
struct Ctx {
    session: SessionSignal,
    message: RwSignal<Option<String>>,
    tab: RwSignal<Tab>,
    filter: RwSignal<Filter>,
    selected: RwSignal<Option<Element>>,
    /// The molecule chip whose card is pinned open: (reaction, position).
    pinned: RwSignal<Option<(Element, usize)>>,
    /// The reaction to go back to after jumping from one of its molecule
    /// chips to the Molecules list.
    return_to: RwSignal<Option<Element>>,
    /// Whose proteins UniProt searches cover (an NCBI taxon id), `None`
    /// for any organism. Starts as the model's organism, if it says.
    organism: RwSignal<Option<u32>>,
}

/// Opens an element in its tab and scrolls it into view.
fn reveal(ctx: Ctx, el: Element) {
    ctx.tab.set(match el {
        Element::Reaction(_) => Tab::Reactions,
        Element::Compartment(_) => Tab::Compartments,
        _ => Tab::Molecules,
    });
    ctx.filter.set(Filter::All);
    ctx.selected.set(Some(el));
    set_timeout(
        move || {
            if let Some(e) = document().get_element_by_id(&dom_id(el)) {
                e.scroll_into_view();
            }
        },
        Duration::from_millis(50),
    );
}

fn dom_id(el: Element) -> String {
    match el {
        Element::Species(i) => format!("species-{i}"),
        Element::Reaction(i) => format!("reaction-{i}"),
        Element::Compartment(i) => format!("compartment-{i}"),
        Element::Model => "model".into(),
    }
}

fn level_text(level: Confidence) -> &'static str {
    match level {
        Confidence::High => "high confidence",
        Confidence::Likely => "likely",
        Confidence::Uncertain => "uncertain",
    }
}

/// A suggestion's name as a link to its database entry (new tab), so people
/// can check it themselves; plain bold text if there is nowhere to go.
/// Clicking it does not open or close the row it is in.
fn check_link(label: String, url: Option<String>) -> AnyView {
    match url {
        Some(url) => view! {
            <a
                class="check"
                href=url
                target="_blank"
                rel="noopener noreferrer"
                title="Check this in its database (opens a new tab)"
                on:click=|ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {label}
                <svg class="ext" viewBox="0 0 12 12" width="11" height="11" aria-hidden="true">
                    <path d="M5 2H2v8h8V7M7 1h4v4M11 1L5.5 6.5" fill="none" stroke="currentColor" stroke-width="1.3"/>
                </svg>
            </a>
        }
            .into_any(),
        None => view! { <b>{label}</b> }.into_any(),
    }
}

#[component]
pub fn App() -> impl IntoView {
    let pack: RwSignal<Option<Rc<kpack::Pack>>, LocalStorage> = RwSignal::new_local(None);
    let pack_error = RwSignal::new(None::<String>);
    let ctx = Ctx {
        session: RwSignal::new_local(None),
        message: RwSignal::new(None),
        tab: RwSignal::new(Tab::Molecules),
        filter: RwSignal::new(Filter::All),
        selected: RwSignal::new(None),
        pinned: RwSignal::new(None),
        return_to: RwSignal::new(None),
        organism: RwSignal::new(None),
    };
    provide_context(ctx);

    spawn_local(async move {
        match browser::fetch_pack("kpack.bin").await {
            Ok(p) => pack.set(Some(Rc::new(p))),
            Err(e) => pack_error.set(Some(e)),
        }
    });

    let open = move |text: String, name: String| {
        let Some(p) = pack.get_untracked() else {
            ctx.message.set(Some("The knowledge base is still loading; try again in a moment.".into()));
            return;
        };
        match annot_model::load(&text) {
            Ok(ws) => {
                ctx.organism.set(ws.model_taxon());
                let mut session = Session::new(p, ws, name);
                session.remember_online(browser::remembered_names());
                ctx.session.set(Some(session));
                ctx.selected.set(None);
                ctx.filter.set(Filter::All);
                ctx.message.set(None);
            }
            Err(e) => ctx.message.set(Some(format!("Could not open {name}: {e}."))),
        }
    };

    let pack_ready = Memo::new(move |_| pack.with(|p| p.is_some()));
    let has_session = Memo::new(move |_| ctx.session.with(|s| s.is_some()));

    view! {
        <Header/>
        <main>
            {move || {
                if let Some(e) = pack_error.get() {
                    view! {
                        <p class="error">
                            "The knowledge base could not be loaded (" {e} "). Reload the page to try again."
                        </p>
                    }
                        .into_any()
                } else if !pack_ready.get() {
                    view! { <p class="loading">"Loading the knowledge base..."</p> }.into_any()
                } else if !has_session.get() {
                    view! { <Start open=open/> }.into_any()
                } else {
                    view! {
                        <Overview/>
                        <Tabs/>
                    }
                        .into_any()
                }
            }}
            <Message/>
        </main>
    }
}

#[component]
fn Header() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let file = move || s.with(|s| s.as_ref().map(|s| s.file_name.clone()));
    let can_undo = move || s.with(|s| s.as_ref().is_some_and(|s| s.ws.can_undo()));
    let can_redo = move || s.with(|s| s.as_ref().is_some_and(|s| s.ws.can_redo()));
    let save = move |_| {
        let out = s.with_untracked(|s| s.as_ref().map(|s| (s.export_name(), s.ws.to_sbml())));
        if let Some((name, text)) = out {
            match browser::save_file(&name, &text) {
                Ok(()) => ctx.message.set(Some(format!("Saved {name}."))),
                Err(e) => ctx.message.set(Some(format!("Could not save: {e}."))),
            }
        }
    };
    view! {
        <header class="top">
            <h1>"PathwayAnnotator"</h1>
            <span class="file">{file}</span>
            <Show when=move || file().is_some()>
                <div class="actions">
                    <button on:click=move |_| s.update(|s| if let Some(s) = s { s.undo() }) disabled=move || !can_undo()>
                        "Undo"
                    </button>
                    <button on:click=move |_| s.update(|s| if let Some(s) = s { s.redo() }) disabled=move || !can_redo()>
                        "Redo"
                    </button>
                    <button class="primary" on:click=save>"Save annotated SBML"</button>
                    <button on:click=move |_| s.set(None)>"Close"</button>
                </div>
            </Show>
        </header>
    }
}

#[component]
fn Start<F>(open: F) -> impl IntoView
where
    F: Fn(String, String) + Copy + Send + Sync + 'static,
{
    let ctx = expect_context::<Ctx>();
    let dragging = RwSignal::new(false);
    let read = move |file: web_sys::File| {
        spawn_local(async move {
            match browser::read_file(&file).await {
                Ok(text) => open(text, file.name()),
                Err(e) => ctx.message.set(Some(format!("Could not read {}: {e}.", file.name()))),
            }
        });
    };
    view! {
        <section class="start">
            <h2>"Annotate a model"</h2>
            <p>
                "Open an SBML or Antimony model. The annotator suggests what each molecule and reaction is, "
                "with the evidence, for you to confirm. Your model stays on this computer."
            </p>
            <label
                class="drop"
                class:dragging=move || dragging.get()
                on:dragover=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    dragging.set(true);
                }
                on:dragleave=move |_| dragging.set(false)
                on:drop=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    dragging.set(false);
                    if let Some(file) = ev.data_transfer().and_then(|d| d.files()).and_then(|f| f.get(0)) {
                        read(file);
                    }
                }
            >
                <input
                    type="file"
                    accept=".xml,.sbml,.ant,.txt"
                    on:change=move |ev| {
                        let input: web_sys::HtmlInputElement = event_target(&ev);
                        if let Some(file) = input.files().and_then(|f| f.get(0)) {
                            read(file);
                        }
                    }
                />
                <span class="drop-text">"Drop a model file here, or click to choose one"</span>
            </label>
            <p class="or">
                "No model at hand? "
                <button class="link" on:click=move |_| open(EXAMPLE.to_string(), "glycolysis-example.ant".into())>
                    "Try the glycolysis example"
                </button>
            </p>
        </section>
    }
}

#[component]
fn Overview() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let counts = move || {
        s.with(|s| {
            s.as_ref()
                .map(|s| (s.counts(&s.species()), s.counts(&s.reactions())))
                .unwrap_or_default()
        })
    };
    let high = move || {
        let (a, b) = counts();
        a.high + b.high
    };
    let accept_high = move |_| {
        let mut n = 0;
        s.update(|s| {
            if let Some(s) = s {
                n = s.accept_all_high();
            }
        });
        ctx.message.set(Some(format!("Accepted {n} high-confidence suggestions. Undo reverts all of them in one step.")));
    };
    view! {
        <section class="overview">
            <Progress title="Molecules" counts=Signal::derive(move || counts().0)/>
            <Progress title="Reactions" counts=Signal::derive(move || counts().1)/>
            <div class="bulk">
                <button class="primary" disabled=move || high() == 0 on:click=accept_high>
                    {move || format!("Accept all high-confidence suggestions ({})", high())}
                </button>
            </div>
        </section>
    }
}

#[component]
fn Progress(title: &'static str, counts: Signal<Counts>) -> impl IntoView {
    let pct = move |n: usize| {
        let c = counts.get();
        if c.total == 0 { 0.0 } else { 100.0 * n as f64 / c.total as f64 }
    };
    view! {
        <div class="progress">
            <div class="progress-head">
                <b>{title}</b>
                <span>{move || {
                    let c = counts.get();
                    format!("{} of {} identified", c.done, c.total)
                }}</span>
            </div>
            <div class="bar" role="img" aria-label=move || {
                let c = counts.get();
                format!("{} identified, {} high, {} likely, {} uncertain, {} with no suggestion", c.done, c.high, c.likely, c.uncertain, c.unknown)
            }>
                <span class="seg done" style=move || format!("width:{}%", pct(counts.get().done))></span>
                <span class="seg high" style=move || format!("width:{}%", pct(counts.get().high))></span>
                <span class="seg likely" style=move || format!("width:{}%", pct(counts.get().likely))></span>
                <span class="seg uncertain" style=move || format!("width:{}%", pct(counts.get().uncertain))></span>
            </div>
            <div class="legend">
                <span><i class="dot done"></i>{move || format!("identified {}", counts.get().done)}</span>
                <span><i class="dot high"></i>{move || format!("high {}", counts.get().high)}</span>
                <span><i class="dot likely"></i>{move || format!("likely {}", counts.get().likely)}</span>
                <span><i class="dot uncertain"></i>{move || format!("uncertain {}", counts.get().uncertain)}</span>
                <span><i class="dot unknown"></i>{move || format!("no suggestion {}", counts.get().unknown)}</span>
            </div>
        </div>
    }
}

#[component]
fn Tabs() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let tab_button = move |t: Tab, label: &'static str| {
        view! {
            <button
                role="tab"
                class="tab"
                class:active=move || ctx.tab.get() == t
                aria-selected=move || (ctx.tab.get() == t).to_string()
                on:click=move |_| ctx.tab.set(t)
            >
                {label}
            </button>
        }
    };
    let filter_button = move |f: Filter, label: &'static str| {
        view! {
            <button class="chip-filter" class:active=move || ctx.filter.get() == f on:click=move |_| ctx.filter.set(f)>
                {label}
            </button>
        }
    };
    view! {
        <nav class="tabs" role="tablist">
            {tab_button(Tab::Molecules, "Molecules")}
            {tab_button(Tab::Reactions, "Reactions")}
            {tab_button(Tab::Compartments, "Compartments")}
            <span class="filters">
                {filter_button(Filter::All, "All")}
                {filter_button(Filter::Review, "To review")}
                {filter_button(Filter::Done, "Identified")}
                {filter_button(Filter::Nothing, "No suggestion")}
            </span>
        </nav>
        <ItemList/>
    }
}

#[component]
fn ItemList() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let items = move || {
        let tab = ctx.tab.get();
        let filter = ctx.filter.get();
        ctx.session.with(|s| {
            let Some(s) = s else { return Vec::new() };
            let all = match tab {
                Tab::Molecules => s.species(),
                Tab::Reactions => s.reactions(),
                Tab::Compartments => s.compartments(),
            };
            all.into_iter().filter(|el| filter.keeps(&s.status(*el))).collect::<Vec<_>>()
        })
    };
    // After jumping from a reaction's molecule chip: the way back.
    let back = move || {
        let r = ctx.return_to.get()?;
        if ctx.tab.get() != Tab::Molecules {
            return None;
        }
        let name = ctx.session.with(|s| s.as_ref().map(|s| s.display_name(r)))?;
        Some(view! {
            <div class="back">
                <button on:click=move |_| {
                    ctx.return_to.set(None);
                    reveal(ctx, r);
                }>
                    <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                        <path d="M7.5 2.5L4 6l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.5"/>
                    </svg>
                    {format!(" Back to reaction {name}")}
                </button>
            </div>
        })
    };
    view! {
        {back}
        <ul class="items">
            <For each=items key=|el| *el children=move |el| view! { <Item el=el/> }/>
        </ul>
        <Show when=move || items().is_empty()>
            <p class="empty">"Nothing here with this filter."</p>
        </Show>
    }
}

#[component]
fn Item(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let name = move || s.with(|s| s.as_ref().map(|s| s.display_name(el)).unwrap_or_default());
    let id = move || {
        s.with(|s| {
            s.as_ref()
                .map(|s| s.ws.element_id(el).to_string())
                .filter(|id| *id != name())
                .unwrap_or_default()
        })
    };
    let status = move || s.with(|s| s.as_ref().map(|s| s.status(el)).unwrap_or(Status::Unknown));
    let open = move || ctx.selected.get() == Some(el);
    let toggle = move |_| ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
    // A compartment's suggestion is a GO term from the pack's table of
    // compartment names, not a pack entry.
    let compartment = matches!(el, Element::Compartment(_));
    let accept_first = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            let Some(s) = s else { return };
            if compartment {
                let msg = s.accept_compartment(el);
                ctx.message.set(Some(msg));
            } else if let Some(c) = s.candidates(el).first() {
                let msg = s.accept(el, c.target);
                ctx.message.set(Some(msg));
            }
        });
    };
    let reject_first = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            let Some(s) = s else { return };
            if compartment {
                let msg = s.reject_compartment(el);
                ctx.message.set(Some(msg));
            } else if let Some(c) = s.candidates(el).first() {
                let msg = s.reject(el, c.target);
                ctx.message.set(Some(msg));
            }
        });
    };
    // Where to check the suggestion, or the current identification.
    let suggestion_link = move || {
        s.with(|s| {
            s.as_ref().and_then(|s| {
                if compartment {
                    return s.compartment_suggestion(el).map(|(uri, _)| uri);
                }
                s.candidates(el).first().and_then(|c| s.main_link(c.target))
            })
        })
    };
    let identity_link = move || s.with(|s| s.as_ref().and_then(|s| s.ws.identity_uris(el).into_iter().next()));
    // The most familiar other name of the suggestion: "D-glucopyranose,
    // also glucose".
    let suggestion_aka = move || {
        s.with(|s| {
            s.as_ref().and_then(|s| {
                s.candidates(el)
                    .first()
                    .and_then(|c| s.synonyms(c.target).into_iter().next())
            })
        })
    };
    let what = move || match status() {
        Status::Identified { label } => view! {
            <span class="what">"Identified as " {check_link(label, identity_link())}</span>
        }
            .into_any(),
        Status::AnnotatedElsewhere { label: Some(label) } => {
            view! { <span class="what">"Identified as " {check_link(label, identity_link())}</span> }.into_any()
        }
        Status::AnnotatedElsewhere { label: None } => {
            view! { <span class="what">"Annotated (with something outside our database)"</span> }.into_any()
        }
        Status::Suggested { label, level } => view! {
            <span class="what">
                {check_link(label, suggestion_link())}
                {suggestion_aka().map(|a| view! { <span class="aka">"also " {a}</span> })}
                <span class=format!("badge {}", Status::Suggested { label: String::new(), level }.class())>
                    {level_text(level)}
                </span>
            </span>
            <button class="accept" on:click=accept_first>"Accept"</button>
            <button class="reject" title="Not this: do not suggest it again" on:click=reject_first>
                "Reject"
            </button>
        }
            .into_any(),
        Status::Unknown => {
            view! { <span class="what muted">"No suggestion: search other databases in Details"</span> }.into_any()
        }
    };
    view! {
        <li class="item" class:open=open id=dom_id(el)>
            <div class="row" on:click=toggle>
                <span class=move || format!("dot {}", status().class())></span>
                <span class="name">{name}</span>
                <span class="id">{id}</span>
                {what}
                // An explicit control: the whole row opens too, but nothing
                // else says so.
                <button
                    class="disclose"
                    aria-expanded=move || open().to_string()
                    title="Show the suggestions, their evidence and the current annotation"
                    on:click=move |ev: web_sys::MouseEvent| {
                        ev.stop_propagation();
                        ctx.selected.update(|sel| *sel = if *sel == Some(el) { None } else { Some(el) });
                    }
                >
                    {move || if open() { "Hide" } else { "Details" }}
                    <svg class="chevron" viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                        <path d="M3 4.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.5"/>
                    </svg>
                </button>
            </div>
            {matches!(el, Element::Reaction(_)).then(|| view! { <Equation el=el/> })}
            <Show when=open>
                <Details el=el/>
            </Show>
        </li>
    }
}

/// A reaction as "A + B -> C", each molecule a chip coloured by its status.
/// Hovering (or tabbing to) a chip shows a card about the molecule; clicking
/// pins the card open. The card can accept the molecule's suggestion in
/// place, so checking a reaction's molecules never leaves the reaction.
#[component]
fn Equation(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let Element::Reaction(ri) = el else { return ().into_any() };
    // (position in the reaction, species, label, status class)
    let side = move |role: Role| {
        ctx.session.with(|s| {
            let Some(s) = s else { return Vec::new() };
            s.model.reactions[ri]
                .participants
                .iter()
                .enumerate()
                .filter(|(_, p)| p.role == role)
                .map(|(k, p)| {
                    let sp = Element::Species(p.species);
                    let n = if p.stoichiometry == 1.0 { String::new() } else { format!("{} ", p.stoichiometry) };
                    (k, sp, format!("{n}{}", s.display_name(sp)), s.status(sp).class())
                })
                .collect::<Vec<_>>()
        })
    };
    let join = move |items: Vec<(usize, Element, String, &'static str)>| {
        let n = items.len();
        items
            .into_iter()
            .enumerate()
            .map(|(i, (k, sp, label, class))| {
                view! {
                    <Chip reaction=el position=k species=sp label=label class=class/>
                    {(i + 1 < n).then_some(" + ")}
                }
            })
            .collect_view()
    };
    view! {
        <div class="equation">
            {move || join(side(Role::Reactant))}
            <span class="arrow">" -> "</span>
            {move || join(side(Role::Product))}
            {move || {
                let m = side(Role::Modifier);
                (!m.is_empty()).then(|| view! { <span class="modifiers">"  (modified by " {join(m)} ")"</span> })
            }}
        </div>
    }
        .into_any()
}

/// One molecule in a reaction's equation, with its card.
#[component]
fn Chip(reaction: Element, position: usize, species: Element, label: String, class: &'static str) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let key = (reaction, position);
    let pinned = move || ctx.pinned.get() == Some(key);
    let card_id = format!("card-{}-{position}", dom_id(reaction));
    // Where the card opens: below and to the right of the chip unless there
    // is no room there ([`place_card`]).
    let wrap_ref = NodeRef::<leptos::html::Span>::new();
    let card_ref = NodeRef::<leptos::html::Span>::new();
    let up = RwSignal::new(false);
    let end = RwSignal::new(false);
    let place = move || {
        request_animation_frame(move || {
            if let (Some(wrap), Some(card)) = (wrap_ref.get_untracked(), card_ref.get_untracked())
                && let Some((u, e)) = place_card(&wrap, &card)
            {
                up.set(u);
                end.set(e);
            }
        })
    };
    let info = move || s.with(|s| s.as_ref().map(|s| s.molecule_info(species)));
    let accept = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            if let Some(s) = s
                && let Some(t) = s.molecule_info(species).target
            {
                let msg = s.accept(species, t);
                ctx.message.set(Some(msg));
            }
        });
    };
    let reject = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        s.update(|s| {
            if let Some(s) = s
                && let Some(t) = s.molecule_info(species).target
            {
                let msg = s.reject(species, t);
                ctx.message.set(Some(msg));
            }
        });
    };
    let show_in_list = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        ctx.pinned.set(None);
        ctx.return_to.set(Some(reaction));
        reveal(ctx, species);
    };
    let card = move || {
        let Some(i) = info() else { return ().into_any() };
        let status = match &i.status {
            Status::Identified { label } => view! { <p>"Identified as " <b>{label.clone()}</b></p> }.into_any(),
            Status::AnnotatedElsewhere { label: Some(label) } => {
                view! { <p>"Identified as " <b>{label.clone()}</b></p> }.into_any()
            }
            Status::AnnotatedElsewhere { label: None } => {
                view! { <p>"Annotated with something outside our database"</p> }.into_any()
            }
            Status::Suggested { label, level } => view! {
                <p>
                    "Suggested: " <b>{label.clone()}</b>
                    <span class=format!("badge {}", i.status.class())>{level_text(*level)}</span>
                </p>
            }
                .into_any(),
            Status::Unknown => view! { <p class="muted">"No suggestion"</p> }.into_any(),
        };
        let suggested = matches!(i.status, Status::Suggested { .. });
        view! {
            <div class="chip-card-body">
                <div class="chip-card-title">
                    <b>{i.name.clone()}</b>
                    {i.id.clone().map(|id| view! { <span class="id">{id}</span> })}
                    {i.compartment.clone().map(|c| view! { <span class="muted">" in " {c}</span> })}
                </div>
                {status}
                {(!i.synonyms.is_empty())
                    .then(|| {
                        let few: Vec<String> = i.synonyms.iter().take(4).cloned().collect();
                        view! { <p class="aka-list">"Also known as: " {few.join("; ")}</p> }
                    })}
                <div class="chip-card-actions">
                    {i.link.clone().map(|url| check_link("Check in database".into(), Some(url)))}
                    {suggested
                        .then(|| {
                            view! {
                                <button class="accept" on:click=accept>"Accept"</button>
                                <button class="reject" title="Not this: do not suggest it again" on:click=reject>
                                    "Reject"
                                </button>
                            }
                        })}
                    <button class="link" on:click=show_in_list>"Show in Molecules list"</button>
                </div>
            </div>
        }
            .into_any()
    };
    view! {
        <span
            class="chip-wrap"
            class:pinned=pinned
            class:up=move || up.get()
            class:end=move || end.get()
            node_ref=wrap_ref
            on:mouseenter=move |_| place()
            on:focusin=move |_| place()
        >
            <button
                class=format!("chip {class}")
                aria-expanded=move || pinned().to_string()
                aria-controls=card_id.clone()
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.stop_propagation();
                    ctx.pinned.update(|p| *p = if *p == Some(key) { None } else { Some(key) });
                    place();
                }
            >
                {label}
            </button>
            <span
                class="chip-card"
                id=card_id.clone()
                node_ref=card_ref
                on:click=|ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {card}
            </span>
        </span>
    }
}

/// Where a chip's card fits, once it is showing: `(up, end)`, opening
/// upward when there is not room below the chip but more above, and lining
/// up with the chip's right edge when it would run off the right of the
/// window. `None` when the card is not showing (nothing to measure).
fn place_card(wrap: &web_sys::HtmlElement, card: &web_sys::HtmlElement) -> Option<(bool, bool)> {
    let c = card.get_bounding_client_rect();
    if c.height() == 0.0 {
        return None;
    }
    let w = wrap.get_bounding_client_rect();
    let root = document().document_element()?;
    let (view_w, view_h) = (f64::from(root.client_width()), f64::from(root.client_height()));
    let (below, above) = (view_h - w.bottom(), w.top());
    let up = c.height() > below && above > below;
    let end = w.left() + c.width() > view_w && w.right() - c.width() >= 0.0;
    Some((up, end))
}

/// An element's SBML id, and renaming it: every reference in the model
/// follows (reactions, rate laws, rules, events, layouts), as one undoable
/// step, or nothing changes and the reason is shown.
#[component]
fn RenameId(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    let current = move || s.with(|s| s.as_ref().map(|s| s.ws.element_id(el).to_string()).unwrap_or_default());
    let start = move |_| {
        draft.set(current());
        error.set(None);
        editing.set(true);
    };
    let cancel = move || {
        editing.set(false);
        error.set(None);
    };
    let commit = move || {
        let new_id = draft.get_untracked();
        let mut result = None;
        s.update(|s| {
            if let Some(s) = s {
                result = Some(s.rename(el, &new_id));
            }
        });
        match result {
            Some(Ok(msg)) => {
                editing.set(false);
                error.set(None);
                ctx.message.set(Some(msg));
            }
            Some(Err(e)) => error.set(Some(e)),
            None => {}
        }
    };
    let input_id = format!("rename-{}", dom_id(el));
    // Focus the field, with its text selected, when it opens: the user
    // types the new id straight away. (The autofocus attribute only works
    // for elements present when the page loads.)
    let input_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    Effect::new(move |_| {
        if editing.get()
            && let Some(input) = input_ref.get()
        {
            let _ = input.focus();
            input.select();
        }
    });
    view! {
        <div class="rename">
            <Show
                when=move || editing.get()
                fallback=move || {
                    view! {
                        <span class="muted">"SBML id: "</span>
                        <code>{current}</code>
                        <button class="link" on:click=start>"Rename"</button>
                    }
                }
            >
                <label for=input_id.clone()>"New SBML id"</label>
                <input
                    node_ref=input_ref
                    id=input_id.clone()
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    prop:value=move || draft.get()
                    on:input=move |ev| draft.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
                        "Enter" => commit(),
                        "Escape" => cancel(),
                        _ => {}
                    }
                />
                <button class="primary" on:click=move |_| commit()>"Rename"</button>
                <button on:click=move |_| cancel()>"Cancel"</button>
                <p class="hint">
                    "Every reference to it in the model is updated too: reactions, rate laws, rules, events and layout."
                </p>
                {move || error.get().map(|e| view! { <p class="error" role="alert">{e}</p> })}
            </Show>
        </div>
    }
}

/// Choosing whose proteins UniProt searches cover. Shared by every search
/// (it is part of [`Ctx`]); the model's own organism is offered first when
/// it names one.
#[component]
fn OrganismChoice() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let model_taxon = ctx.session.with_untracked(|s| s.as_ref().and_then(|s| s.ws.model_taxon()));
    let mut options: Vec<(u32, String)> = Vec::new();
    if let Some(t) = model_taxon {
        let name = annot_online::ORGANISMS.iter().find(|(id, _)| *id == t).map(|(_, n)| n.to_string());
        options.push((t, format!("{} (the model's)", name.unwrap_or_else(|| format!("taxon {t}")))));
    }
    for (id, name) in annot_online::ORGANISMS {
        if Some(*id) != model_taxon {
            options.push((*id, name.to_string()));
        }
    }
    view! {
        <label class="organism">
            <span class="muted">"Proteins from "</span>
            <select on:change=move |ev| ctx.organism.set(event_target_value(&ev).parse().ok())>
                <option value="" selected=move || ctx.organism.get().is_none()>"any organism"</option>
                {options
                    .into_iter()
                    .map(|(id, name)| {
                        view! {
                            <option value=id.to_string() selected=move || ctx.organism.get() == Some(id)>
                                {name}
                            </option>
                        }
                    })
                    .collect_view()}
            </select>
        </label>
    }
}

/// How many proteins to show when none has exactly the name searched for.
const PROTEINS_SHOWN: usize = 3;

/// What a search of other databases has come to.
#[derive(Clone)]
enum Found {
    Searching,
    /// The entries, best first, and anything that went wrong (one service
    /// failing does not hide the other's answer).
    Done {
        hits: Vec<annot_online::Hit>,
        problems: Vec<String>,
        /// A note on how the search went ("no reaction has all of them").
        note: Option<String>,
    },
}

/// Searching databases beyond the knowledge base, for a molecule or
/// reaction the annotator knows nothing (or not enough) about: ChEBI and
/// UniProt for a molecule, Rhea for a reaction. Nothing is sent anywhere
/// until the user presses Search; what is sent, and to whom, is said
/// beside the button.
#[component]
fn OnlineSearch(el: Element) -> impl IntoView {
    use annot_online::{Db, RheaQuery};
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let reaction = matches!(el, Element::Reaction(_));
    let compartment = matches!(el, Element::Compartment(_));
    let text = RwSignal::new(s.with_untracked(|s| s.as_ref().map(|s| s.search_text(el)).unwrap_or_default()));
    let enzyme = RwSignal::new(s.with_untracked(|s| s.as_ref().map(|s| s.enzyme_text(el)).unwrap_or_default()));
    let found = RwSignal::new(None::<Found>);
    // For a reaction: its molecules, and which are identified (the search
    // uses their ChEBI ids, else their names).
    let parts = move || s.with(|s| s.as_ref().map(|s| s.rhea_participants(el)).unwrap_or_default());

    let search_molecule = move || {
        let q = text.get_untracked().trim().to_string();
        if q.is_empty() {
            return;
        }
        let taxon = ctx.organism.get_untracked();
        found.set(Some(Found::Searching));
        spawn_local(async move {
            let mut hits = Vec::new();
            let mut problems = Vec::new();
            if compartment {
                match browser::fetch_text(&annot_online::go_url(&q)).await.and_then(|j| annot_online::parse_go(&j, &q)) {
                    Ok(mut h) => {
                        annot_online::exact_first(&mut h);
                        hits.extend(h);
                    }
                    Err(e) => problems.push(format!("GO: {e}")),
                }
                found.set(Some(Found::Done { hits, problems, note: None }));
                return;
            }
            match browser::fetch_text(&annot_online::chebi_url(&q)).await.and_then(|j| annot_online::parse_chebi(&j, &q)) {
                Ok(mut h) => {
                    annot_online::exact_first(&mut h);
                    hits.extend(h);
                }
                Err(e) => problems.push(format!("ChEBI: {e}")),
            }
            let mut note = None;
            match browser::fetch_text(&annot_online::uniprot_url(&q, taxon))
                .await
                .and_then(|t| annot_online::parse_uniprot(&t, &q))
            {
                Ok(mut h) => {
                    annot_online::exact_first(&mut h);
                    // Proteins that only mention the words (a molecule's
                    // name in an enzyme's) are noise: a few at most.
                    if !h.iter().any(|h| h.exact) && h.len() > PROTEINS_SHOWN {
                        h.truncate(PROTEINS_SHOWN);
                        note = Some(format!(
                            "No protein has exactly this name; showing the first {PROTEINS_SHOWN} UniProt finds."
                        ));
                    }
                    hits.extend(h);
                }
                Err(e) => problems.push(format!("UniProt: {e}")),
            }
            found.set(Some(Found::Done { hits, problems, note }));
        });
    };
    let search_reaction = move || {
        let parts = parts();
        let Some(query) = RheaQuery::for_reaction(&parts) else {
            found.set(Some(Found::Done {
                hits: Vec::new(),
                problems: vec!["This reaction has no molecules to search by.".into()],
                note: None,
            }));
            return;
        };
        found.set(Some(Found::Searching));
        spawn_local(async move {
            let ranked = |q: annot_online::RheaQuery| {
                let parts = parts.clone();
                async move {
                    let tsv = browser::fetch_text(&annot_online::rhea_url(&q)).await?;
                    s.with_untracked(|s| match s {
                        Some(s) => annot_online::rank_rhea(&tsv, &parts, |id| s.chebi_is_currency(id)),
                        None => Ok(Vec::new()),
                    })
                }
            };
            let mut note = None;
            let mut result = ranked(query.clone()).await;
            // Nothing has all of them: one may be identified with an entry
            // Rhea does not use. Try leaving each out in turn.
            if matches!(&result, Ok(h) if h.is_empty()) && !query.relaxed().is_empty() {
                let mut lists = Vec::new();
                for q in query.relaxed() {
                    match ranked(q).await {
                        Ok(h) => lists.push(h),
                        Err(e) => {
                            result = Err(e);
                            break;
                        }
                    }
                }
                if result.is_ok() {
                    note = Some("No Rhea reaction has all of these molecules; these have all but one.".to_string());
                    result = Ok(annot_online::merge_ranked(lists));
                }
            }
            found.set(Some(match result {
                Ok(hits) => Found::Done { hits, problems: Vec::new(), note },
                Err(e) => Found::Done { hits: Vec::new(), problems: vec![format!("Rhea: {e}")], note: None },
            }));
        });
    };
    let search = move || if reaction { search_reaction() } else { search_molecule() };
    // A reaction by its enzyme: an EC number goes straight to Rhea; a name
    // is looked up in UniProt for its EC numbers first.
    let search_enzyme = move || {
        let q = enzyme.get_untracked().trim().to_string();
        if q.is_empty() {
            return;
        }
        let parts = parts();
        let taxon = ctx.organism.get_untracked();
        found.set(Some(Found::Searching));
        spawn_local(async move {
            let done = |hits, problems: Vec<String>, note: Option<String>| {
                found.set(Some(Found::Done { hits, problems, note }));
            };
            let (ecs, from) = match annot_online::ec_from_text(&q) {
                Some(ec) => (vec![ec], None),
                None => {
                    let proteins = browser::fetch_text(&annot_online::uniprot_ec_url(&q, taxon))
                        .await
                        .and_then(|t| annot_online::parse_uniprot(&t, &q));
                    match proteins {
                        Ok(p) => (annot_online::ecs_of_proteins(&p), Some(q.clone())),
                        Err(e) => return done(Vec::new(), vec![format!("UniProt: {e}")], None),
                    }
                }
            };
            if ecs.is_empty() {
                let note = format!(
                    "No reviewed protein named \"{q}\" has an EC number{}. Try another name, or an EC number.",
                    if taxon.is_some() { " in this organism" } else { "" }
                );
                return done(Vec::new(), Vec::new(), Some(note));
            }
            let query = RheaQuery::ByEc(ecs.clone());
            let result = browser::fetch_text(&annot_online::rhea_url(&query)).await.and_then(|tsv| {
                s.with_untracked(|s| match s {
                    Some(s) => annot_online::rank_rhea(&tsv, &parts, |id| s.chebi_is_currency(id)),
                    None => Ok(Vec::new()),
                })
            });
            let ecs = ecs.join(", ");
            let note = match from {
                Some(name) => format!("Rhea reactions of EC {ecs}, the activity of proteins named \"{name}\" in UniProt."),
                None => format!("Rhea reactions of EC {ecs}."),
            };
            match result {
                Ok(hits) => done(hits, Vec::new(), Some(note)),
                Err(e) => done(Vec::new(), vec![format!("Rhea: {e}")], None),
            }
        });
    };
    let organism_choice = move || view! { <OrganismChoice/> };

    let form = move || {
        if reaction {
            let parts = parts();
            let main: Vec<_> = parts.iter().filter(|p| !p.currency).cloned().collect();
            let main = if main.is_empty() { parts } else { main };
            let by_ids = main.iter().any(|p| !p.chebi.is_empty());
            let listed = main
                .iter()
                .map(|p| {
                    let known = !p.chebi.is_empty();
                    view! {
                        <span class="chip" class:done=known class:unknown=!known>
                            {p.name.clone()}
                        </span>
                    }
                })
                .collect_view();
            let how = if by_ids {
                "Looks for Rhea reactions with every identified molecule (green), and ranks them by how well both sides fit."
            } else {
                "None of these molecules is identified yet, so Rhea is searched by their names. Identifying them first gives much better results."
            };
            view! {
                <div class="online-form">
                    <span class="muted">"Molecules: "</span>
                    {listed}
                    <button class="primary" on:click=move |_| search()>"Search Rhea"</button>
                </div>
                <p class="hint">{how} " Sends their ChEBI ids or names to " {Db::Rhea.host()} "."</p>
                <div class="online-form">
                    <label>
                        <span class="muted">"Or by enzyme name or EC number "</span>
                        <input
                            type="text"
                            placeholder="phosphofructokinase, or 2.7.1.11"
                            prop:value=move || enzyme.get()
                            on:input=move |ev| enzyme.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search_enzyme();
                                }
                            }
                        />
                    </label>
                    {organism_choice}
                    <button on:click=move |_| search_enzyme()>"Search by enzyme"</button>
                </div>
                <p class="hint">
                    "A name is looked up in UniProt for the enzyme's EC numbers, then Rhea lists the reactions of those numbers. "
                    "Sends the text to " {Db::Uniprot.host()} " and " {Db::Rhea.host()} "."
                </p>
            }
                .into_any()
        } else if compartment {
            view! {
                <div class="online-form">
                    <label>
                        <span class="muted">"Search GO for "</span>
                        <input
                            type="text"
                            prop:value=move || text.get()
                            on:input=move |ev| text.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search();
                                }
                            }
                        />
                    </label>
                    <button class="primary" on:click=move |_| search()>"Search GO"</button>
                </div>
                <p class="hint">
                    "The Gene Ontology's cellular components: compartments and structures of the cell. "
                    "Sends the text to " {Db::Go.host()} "."
                </p>
            }
                .into_any()
        } else {
            view! {
                <div class="online-form">
                    <label>
                        <span class="muted">"Search for "</span>
                        <input
                            type="text"
                            prop:value=move || text.get()
                            on:input=move |ev| text.set(event_target_value(&ev))
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" {
                                    search();
                                }
                            }
                        />
                    </label>
                    {organism_choice}
                    <button class="primary" on:click=move |_| search()>"Search ChEBI and UniProt"</button>
                </div>
                <p class="hint">
                    "ChEBI has small molecules beyond the knowledge base; UniProt has proteins (enzymes, transporters). "
                    "Sends the text to " {Db::Chebi.host()} " and " {Db::Uniprot.host()} "."
                </p>
            }
                .into_any()
        }
    };

    let results = move || {
        let Some(f) = found.get() else { return ().into_any() };
        let (hits, problems, note) = match f {
            Found::Searching => return view! { <p class="muted" role="status">"Searching..."</p> }.into_any(),
            Found::Done { hits, problems, note } => (hits, problems, note),
        };
        let current = s.with(|s| s.as_ref().map(|s| s.ws.identity_uris(el)).unwrap_or_default());
        let nothing = hits.is_empty() && problems.is_empty();
        let mut last_db = None;
        let rows = hits
            .into_iter()
            .map(|h| {
                let heading = (last_db != Some(h.db)).then(|| {
                    let what = match h.db {
                        Db::Chebi => "ChEBI: molecules",
                        Db::Uniprot => "UniProt: proteins",
                        Db::Rhea => "Rhea: reactions",
                        Db::Go => "GO: cellular components",
                    };
                    view! { <h5>{what}</h5> }
                });
                last_db = Some(h.db);
                let accepted = current.contains(&h.uri);
                let id = h.id.clone();
                let uri = h.uri.clone();
                let detail = h.detail.clone();
                let why = h.why.clone();
                let aka: Vec<String> = h.synonyms.iter().take(4).cloned().collect();
                let label = h.label.clone();
                let exact = h.exact;
                let action = if accepted {
                    view! { <span class="accepted" role="status">" Accepted"</span> }.into_any()
                } else {
                    view! {
                        <button
                            class="accept"
                            title="Annotate the model with this entry"
                            on:click=move |_| {
                                s.update(|s| {
                                    if let Some(s) = s {
                                        let msg = s.accept_online(el, &h);
                                        if let Some((name, label)) = s.online_entry(&h.uri) {
                                            browser::remember_name(&h.uri, &name, &label);
                                        }
                                        ctx.message.set(Some(msg));
                                    }
                                })
                            }
                        >
                            "Accept"
                        </button>
                    }
                        .into_any()
                };
                view! {
                    {heading}
                    <div class="cand online" class:is-accepted=accepted>
                        <div class="cand-head">
                            {check_link(label, Some(uri.clone()))}
                            <span class="ids">
                                <a href=uri target="_blank" rel="noopener noreferrer">{id}</a>
                            </span>
                            {exact.then(|| view! { <span class="badge high">{if reaction { "fits" } else { "same name" }}</span> })}
                            {action}
                        </div>
                        {detail.map(|d| view! { <p class="muted detail">{d}</p> })}
                        {(!aka.is_empty()).then(|| view! { <p class="aka-list">"Also known as: " {aka.join("; ")}</p> })}
                        {why.map(|w| view! { <ul class="evidence"><li>{w}</li></ul> })}
                    </div>
                }
            })
            .collect_view();
        view! {
            {problems.into_iter().map(|p| view! { <p class="error" role="alert">{p}</p> }).collect_view()}
            {nothing.then(|| view! { <p class="muted">"Nothing found. Try another name or spelling."</p> })}
            {note.map(|n| view! { <p class="muted">{n}</p> })}
            {rows}
        }
            .into_any()
    };

    view! {
        <div class="online">
            <h4>"Search other databases"</h4>
            {form}
            {results}
        </div>
    }
}

/// The candidates for an element, with their evidence, and its existing
/// annotation.
#[component]
fn Details(el: Element) -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    let s = ctx.session;
    let rows = move || {
        s.with(|s| {
            let Some(s) = s else { return Vec::new() };
            let current = s.identified_as(el);
            s.candidates(el)
                .iter()
                .take(5)
                .map(|c| {
                    (
                        c.target,
                        s.label(c.target),
                        s.links(c.target),
                        s.synonyms(c.target),
                        c.evidence.iter().map(|e| e.note.clone()).collect::<Vec<_>>(),
                        // Is it the current identification, and is there one?
                        (current.contains(&c.target), !current.is_empty()),
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    let existing = move || s.with(|s| s.as_ref().map(|s| s.ws.terms(el)).unwrap_or_default());
    view! {
        <div class="details">
            <RenameId el=el/>
            <Show when=move || !existing().is_empty()>
                <h4>"Current annotation"</h4>
                <ul class="terms">
                    {move || {
                        existing()
                            .into_iter()
                            .map(|t| {
                                let q = if t.model { format!("model {}", t.qualifier) } else { t.qualifier.clone() };
                                view! {
                                    <li>
                                        <span class="qualifier">{q}</span>
                                        " "
                                        <a href=t.uri.clone() target="_blank" rel="noopener">{t.uri.clone()}</a>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
            </Show>
            <Show when=move || !matches!(el, Element::Compartment(_))>
            <h4>"Suggestions"</h4>
            {move || {
                let rows = rows();
                if rows.is_empty() {
                    return view! { <p class="muted">"The annotator has nothing to suggest for this one."</p> }.into_any();
                }
                rows.into_iter()
                    .enumerate()
                    .map(|(i, (target, label, links, aka, notes, (accepted, identified)))| {
                        let main = links.first().map(|(_, url)| url.clone());
                        let action = if accepted {
                            view! {
                                <span class="accepted" role="status">
                                    <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                                        <path d="M2 6.5l2.5 2.5L10 3.5" fill="none" stroke="currentColor" stroke-width="1.8"/>
                                    </svg>
                                    " Accepted"
                                </span>
                            }
                                .into_any()
                        } else {
                            view! {
                                <span class="cand-actions">
                                    <button
                                        class="accept"
                                        title=if identified {
                                            "Replace the current identification with this one"
                                        } else {
                                            "Annotate the model with this"
                                        }
                                        on:click=move |_| {
                                            s.update(|s| {
                                                if let Some(s) = s {
                                                    let msg = s.accept(el, target);
                                                    ctx.message.set(Some(msg));
                                                }
                                            })
                                        }
                                    >
                                        {if identified { "Use this instead" } else { "Accept" }}
                                    </button>
                                    <button
                                        class="reject"
                                        title="Not this: do not suggest it again"
                                        on:click=move |_| {
                                            s.update(|s| {
                                                if let Some(s) = s {
                                                    let msg = s.reject(el, target);
                                                    ctx.message.set(Some(msg));
                                                }
                                            })
                                        }
                                    >
                                        "Reject"
                                    </button>
                                </span>
                            }
                                .into_any()
                        };
                        view! {
                            <div class="cand" class:is-accepted=accepted>
                                <div class="cand-head">
                                    <span class="rank">{i + 1}</span>
                                    {check_link(label, main)}
                                    <span class="ids">
                                        {links
                                            .into_iter()
                                            .map(|(text, url)| {
                                                view! {
                                                    <a href=url target="_blank" rel="noopener noreferrer">
                                                        {text}
                                                    </a>
                                                }
                                            })
                                            .collect_view()}
                                    </span>
                                    {action}
                                </div>
                                {(!aka.is_empty())
                                    .then(|| view! { <p class="aka-list">"Also known as: " {aka.join("; ")}</p> })}
                                <ul class="evidence">
                                    {notes.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}
                                </ul>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
            </Show>
            <OnlineSearch el=el/>
            {move || {
                let rejected = s.with(|s| s.as_ref().map(|s| s.rejected(el)).unwrap_or_default());
                (!rejected.is_empty())
                    .then(|| {
                        view! {
                            <h4>"Rejected"</h4>
                            <ul class="rejected">
                                {rejected
                                    .into_iter()
                                    .map(|(target, label)| {
                                        view! {
                                            <li>
                                                <span>{label}</span>
                                                <button
                                                    class="link"
                                                    on:click=move |_| {
                                                        s.update(|s| {
                                                            if let Some(s) = s {
                                                                let msg = s.restore(el, target);
                                                                ctx.message.set(Some(msg));
                                                            }
                                                        })
                                                    }
                                                >
                                                    "Restore"
                                                </button>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                    })
            }}
        </div>
    }
}

#[component]
fn Message() -> impl IntoView {
    let ctx = expect_context::<Ctx>();
    view! {
        <Show when=move || ctx.message.with(|m| m.is_some())>
            <div class="message" role="status">
                <span>{move || ctx.message.get().unwrap_or_default()}</span>
                <button class="link" on:click=move |_| ctx.message.set(None)>"Dismiss"</button>
            </div>
        </Show>
    }
}
