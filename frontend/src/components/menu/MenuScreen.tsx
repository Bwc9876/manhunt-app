import { commands, GameSettings, PlayerProfile } from "@/bindings";
import React, { Dispatch, SetStateAction } from "react";
import NavButton from "./NavButton";
import JoinLobby from "./JoinLobby";
import {
    IoAddOutline,
    IoArrowForward,
    IoAccessibilityOutline,
    IoCalendarClearOutline
} from "react-icons/io5";
import EditProfile from "./EditProfile";
import useSWR, { KeyedMutator } from "swr";
import { sharedSwrConfig } from "@/lib/hooks";

// Temp settings for now.
const settings: GameSettings = {
    random_seed: 21341234,
    hiding_time_seconds: 10,
    ping_start: "Instant",
    ping_minutes_interval: 1,
    powerup_start: "Instant",
    powerup_chance: 60,
    powerup_minutes_cooldown: 1,
    powerup_locations: [
        {
            lat: 0,
            long: 0,
            heading: null
        }
    ]
};

export enum MenuState {
    Join,
    Create,
    Profile,
    History
}

export function MenuRouter({ state, profile, setProfile }: { state: MenuState, profile: PlayerProfile, setProfile: KeyedMutator<PlayerProfile> }) {
    switch (state) {
        case MenuState.Join:
            return <JoinLobby settings={settings} />;

        case MenuState.Create:
            return <div>Create</div>;

        case MenuState.Profile:
            return <EditProfile profile={profile} setProfile={setProfile}/>;

        case MenuState.History:
            return <div>History</div>;
    }
    return <></>;
}

function NavBar({
    state,
    setState
}: {
    state: MenuState;
    setState: Dispatch<SetStateAction<MenuState>>;
}) {
    return (
        <div className="w-full h-1/8 flex flex-row border-t-1 justify-center border-gray-300 fixed bottom-0">
            <NavButton current={state} setCurrent={setState} target={MenuState.Join}>
                <IoArrowForward className="size-1/3" />
                Join
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.Create}>
                <IoAddOutline className="size-1/3" />
                Create
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.Profile}>
                <IoAccessibilityOutline className="size-1/3" />
                Profile
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.History}>
                <IoCalendarClearOutline className="size-1/3" />
                History
            </NavButton>
        </div>
    );
}

export default function MenuScreen() {
    const [state, setState] = React.useState(MenuState.Join);

    const { data: profile, mutate: setProfile } = useSWR(
        "fetch-profile",
        commands.getProfile,
        sharedSwrConfig
    );


    return (
        <div className="h-screen v-screen flex flex-col items-center justify-center font-sans">
            <MenuRouter state={state} profile={profile} setProfile={setProfile}></MenuRouter>
            <NavBar state={state} setState={setState}></NavBar>
        </div>
    );
}
