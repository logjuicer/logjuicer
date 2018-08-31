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

import { createStore, applyMiddleware, combineReducers } from 'redux'
import update from 'immutability-helper'
import thunk from 'redux-thunk'
import { fetchStatus, fetchAnomalies, fetchAnomaly } from './api'


const statusReducer = (state = null, action) => {
  switch (action.type) {
    case 'FETCH_STATUS_SUCCESS':
      return action.status
    default:
      return state
  }
}

const anomaliesReducer = (state = null, action) => {
  switch (action.type) {
    case 'FETCH_ANOMALIES_SUCCESS':
      return action.list
    default:
      return state
  }
}

const anomalyReducer = (state = {}, action) => {
  const anomaly = {}
  switch (action.type) {
    case 'FETCH_ANOMALY_SUCCESS':
      anomaly[action.anomalyId] = action.anomaly
      return update(state, {$merge: anomaly})
    default:
      return state
  }
}

function createLogClassifyStore () {
  return createStore(combineReducers({
    status: statusReducer,
    anomalies: anomaliesReducer,
    anomaly: anomalyReducer
  }), applyMiddleware(thunk))
}

// Reducer actions
function fetchStatusAction () {
  return (dispatch) => {
    return fetchStatus()
      .then(response => {
        dispatch({type: 'FETCH_STATUS_SUCCESS', status: response.data})
      })
      .catch(error => {
        throw (error)
      })
  }
}

function fetchAnomaliesAction () {
  return (dispatch) => {
    return fetchAnomalies()
      .then(response => {
        dispatch({type: 'FETCH_ANOMALIES_SUCCESS', list: response.data})
      })
      .catch(error => {
        throw (error)
      })
  }
}

function fetchAnomalyAction (anomalyId) {
  return (dispatch) => {
    return fetchAnomaly(anomalyId)
      .then(response => {
        dispatch({
          type: 'FETCH_ANOMALY_SUCCESS',
          anomalyId: anomalyId,
          anomaly: response.data
        })
      })
      .catch(error => {
        throw (error)
      })
  }
}

export {
  createLogClassifyStore,
  fetchAnomaliesAction,
  fetchAnomalyAction,
  fetchStatusAction,
}
