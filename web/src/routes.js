// Copyright 2018 Red Hat, Inc
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

import Welcome from './pages/Welcome'
import Status from './pages/Status'
import AnomalyList from './pages/List'
import AnomalyView from './pages/View'
import UserReport from './pages/UserReport'

const routes = () => [
  {
    title: 'Welcome',
    to: '/',
    component: Welcome
  },
  {
    title: 'New',
    to: '/new',
    component: UserReport
  },
  {
    title: 'List',
    to: '/list',
    component: AnomalyList
  },
  {
    title: 'Status',
    to: '/status',
    component: Status
  },
  {
    to: '/view/:anomalyId',
    component: AnomalyView
  }
]

export { routes }
